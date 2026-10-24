/* Command-line wrapper around the NBIS WSQ codec, used to make fixtures.
 *
 *   harness e IN.raw WIDTH HEIGHT BITRATE PPI OUT.wsq [COMMENT.txt]
 *   harness d IN.wsq OUT.raw
 *
 * COMMENT.txt holds the comment text NBIS takes, merged into its NISTCOM
 * comment when it starts with NIST_COM.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
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

int main(int argc, char **argv)
{
   unsigned char *in, *out;
   int len, olen, w, h, d, ppi, lossy;

   if ((argc == 8 || argc == 9) && argv[1][0] == 'e') {
      char *comment = NULL;
      if (argc == 9) {
         int clen;
         unsigned char *text = slurp(argv[8], &clen);
         comment = calloc(clen + 1, 1);
         memcpy(comment, text, clen);
      }
      in = slurp(argv[2], &len);
      w = atoi(argv[3]);
      h = atoi(argv[4]);
      if (wsq_encode_mem(&out, &olen, atof(argv[5]), in, w, h, 8,
                         atoi(argv[6]), comment))
         return 1;
      spill(argv[7], out, olen);
      return 0;
   }
   if (argc == 4 && argv[1][0] == 'd') {
      in = slurp(argv[2], &len);
      if (wsq_decode_mem(&out, &w, &h, &d, &ppi, &lossy, in, len))
         return 1;
      spill(argv[3], out, w * h);
      return 0;
   }
   fprintf(stderr, "usage: harness e IN W H BITRATE PPI OUT [COMMENT] | d IN OUT\n");
   return 2;
}
