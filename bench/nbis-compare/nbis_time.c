/* Timing harness for NBIS 5.0.0's WSQ codec, the counterpart of src/main.rs
 * (docs/nbis-comparison.md).
 *
 *   nbis_time e RAW WIDTH HEIGHT BITRATE PPI [OUT.wsq]
 *   nbis_time d STREAM.wsq [OUT.raw]
 *
 * One call per process, in memory. Prints `ms=... out_bytes=... fnv=... extra_rss_kb=...`:
 * the time of the call alone (CLOCK_MONOTONIC, file reading and writing excluded), the size
 * and FNV-1a hash of the result, and the peak resident set (VmHWM) minus the resident set just
 * before the call.
 */
#define _POSIX_C_SOURCE 199309L
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <wsq.h>

int debug = 0;

static unsigned char *slurp(const char *path, int *len)
{
   FILE *f = fopen(path, "rb");
   if (f == NULL) {
      perror(path);
      exit(1);
   }
   fseek(f, 0, SEEK_END);
   *len = ftell(f);
   rewind(f);
   unsigned char *buf = malloc(*len);
   if (fread(buf, 1, *len, f) != (size_t)*len) {
      perror(path);
      exit(1);
   }
   fclose(f);
   return buf;
}

static void spill(const char *path, const unsigned char *buf, int len)
{
   FILE *f = fopen(path, "wb");
   if (f == NULL || fwrite(buf, 1, len, f) != (size_t)len) {
      perror(path);
      exit(1);
   }
   fclose(f);
}

static long status_kb(const char *key)
{
   char line[256];
   long value = -1;
   FILE *f = fopen("/proc/self/status", "r");
   if (f == NULL)
      return -1;
   while (fgets(line, sizeof line, f)) {
      if (strncmp(line, key, strlen(key)) == 0) {
         value = atol(line + strlen(key));
         break;
      }
   }
   fclose(f);
   return value;
}

static double now_ms(void)
{
   struct timespec t;
   clock_gettime(CLOCK_MONOTONIC, &t);
   return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

static uint64_t fnv1a(const unsigned char *p, int len)
{
   uint64_t h = 0xcbf29ce484222325ULL;
   for (int i = 0; i < len; i++) {
      h ^= p[i];
      h *= 0x100000001b3ULL;
   }
   return h;
}

static void report(double ms, const unsigned char *out, int len, long before)
{
   long extra = status_kb("VmHWM:") - before;
   if (extra < 0)
      extra = 0;
   printf("ms=%.3f out_bytes=%d fnv=%016llx extra_rss_kb=%ld\n", ms, len,
          (unsigned long long)fnv1a(out, len), extra);
}

int main(int argc, char **argv)
{
   unsigned char *in, *out;
   int len, olen, w, h, d, ppi, lossy;
   long before;
   double start, ms;

   if ((argc == 7 || argc == 8) && argv[1][0] == 'e') {
      in = slurp(argv[2], &len);
      w = atoi(argv[3]);
      h = atoi(argv[4]);
      before = status_kb("VmRSS:");
      start = now_ms();
      if (wsq_encode_mem(&out, &olen, (float)atof(argv[5]), in, w, h, 8,
                         atoi(argv[6]), NULL))
         return 1;
      ms = now_ms() - start;
      report(ms, out, olen, before);
      if (argc == 8)
         spill(argv[7], out, olen);
      return 0;
   }
   if ((argc == 3 || argc == 4) && argv[1][0] == 'd') {
      in = slurp(argv[2], &len);
      before = status_kb("VmRSS:");
      start = now_ms();
      if (wsq_decode_mem(&out, &w, &h, &d, &ppi, &lossy, in, len))
         return 1;
      ms = now_ms() - start;
      report(ms, out, w * h, before);
      if (argc == 4)
         spill(argv[3], out, w * h);
      return 0;
   }
   fprintf(stderr, "usage: nbis_time e RAW W H BITRATE PPI [OUT] | d WSQ [OUT]\n");
   return 2;
}
