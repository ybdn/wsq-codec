Output size and hash identical across every timed call and implementation: yes

### encode

| image | bpp | implementation | n | min ms | median ms | p90 ms | max ms | median / NBIS | median extra RSS MiB |
|---|---|---|---|---|---|---|---|---|---|
| dark_ridges_5500x8000 | 0.75 | NBIS 5.0.0 | 11 | 2528.7 | 2699.9 | 2821.6 | 2864.8 | 1.00 | 335.6 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 1 thread | 11 | 1125.3 | 1326.3 | 1438.5 | 1482.7 | 0.49 | 350.3 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 2 threads | 11 | 818.0 | 962.5 | 1239.2 | 1295.4 | 0.36 | 353.3 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 4 threads | 11 | 677.9 | 862.6 | 1153.8 | 1293.1 | 0.32 | 359.1 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 8 threads | 11 | 659.1 | 930.1 | 974.4 | 1255.9 | 0.34 | 370.5 |
| dark_ridges_5500x8000 | 2.25 | NBIS 5.0.0 | 11 | 3102.8 | 3380.7 | 3585.2 | 3607.3 | 1.00 | 335.6 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 1 thread | 11 | 1621.5 | 1838.2 | 1917.4 | 1931.3 | 0.54 | 350.3 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 2 threads | 11 | 1041.0 | 1105.3 | 1221.6 | 1569.6 | 0.33 | 353.4 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 4 threads | 11 | 974.9 | 1131.4 | 1347.3 | 1504.8 | 0.33 | 359.1 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 8 threads | 11 | 924.2 | 1109.1 | 1275.5 | 1501.1 | 0.33 | 370.5 |
| noise_1600x1500 | 0.75 | NBIS 5.0.0 | 31 | 105.2 | 142.0 | 181.3 | 223.2 | 1.00 | 18.2 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 50.9 | 62.7 | 107.8 | 117.5 | 0.44 | 22.1 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 2 threads | 31 | 55.6 | 73.8 | 101.1 | 126.8 | 0.52 | 23.0 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 54.7 | 64.6 | 79.5 | 101.1 | 0.46 | 23.9 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 8 threads | 31 | 53.3 | 64.9 | 82.7 | 99.2 | 0.46 | 24.8 |
| noise_1600x1500 | 2.25 | NBIS 5.0.0 | 31 | 132.0 | 165.0 | 215.7 | 275.8 | 1.00 | 18.2 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 74.9 | 83.6 | 111.7 | 129.5 | 0.51 | 22.1 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 2 threads | 31 | 75.2 | 87.1 | 113.4 | 147.9 | 0.53 | 23.1 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 66.4 | 87.8 | 109.8 | 132.1 | 0.53 | 24.0 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 8 threads | 31 | 62.0 | 79.9 | 98.0 | 102.8 | 0.48 | 24.9 |
| ridges_1600x1500 | 0.75 | NBIS 5.0.0 | 31 | 106.9 | 153.2 | 250.6 | 269.3 | 1.00 | 18.2 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 50.3 | 79.4 | 131.6 | 179.8 | 0.52 | 22.1 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 2 threads | 31 | 52.5 | 85.2 | 123.9 | 142.0 | 0.56 | 23.0 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 58.2 | 79.4 | 117.1 | 150.8 | 0.52 | 23.8 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 8 threads | 31 | 58.4 | 76.1 | 103.9 | 204.4 | 0.50 | 24.8 |
| ridges_1600x1500 | 2.25 | NBIS 5.0.0 | 31 | 129.4 | 181.4 | 263.0 | 362.3 | 1.00 | 18.2 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 75.8 | 92.1 | 121.6 | 219.6 | 0.51 | 22.1 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 2 threads | 31 | 78.3 | 113.3 | 154.0 | 226.0 | 0.62 | 23.0 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 74.9 | 92.9 | 136.4 | 149.9 | 0.51 | 23.8 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 8 threads | 31 | 66.3 | 97.5 | 123.9 | 151.9 | 0.54 | 24.8 |
| ridges_800x750 | 0.75 | NBIS 5.0.0 | 31 | 22.2 | 48.6 | 100.3 | 115.7 | 1.00 | 4.5 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 12.6 | 22.2 | 56.7 | 69.4 | 0.46 | 6.7 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 2 threads | 31 | 14.0 | 28.0 | 38.1 | 60.1 | 0.58 | 6.8 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 15.2 | 27.1 | 47.1 | 56.7 | 0.56 | 7.3 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 8 threads | 31 | 16.0 | 27.6 | 39.0 | 71.6 | 0.57 | 7.7 |
| ridges_800x750 | 2.25 | NBIS 5.0.0 | 31 | 29.5 | 73.9 | 124.7 | 151.0 | 1.00 | 4.5 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 17.8 | 27.9 | 51.5 | 98.5 | 0.38 | 6.7 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 2 threads | 31 | 20.8 | 32.1 | 52.5 | 69.7 | 0.43 | 6.8 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 20.1 | 34.6 | 52.2 | 109.8 | 0.47 | 7.2 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 8 threads | 31 | 21.4 | 36.1 | 60.9 | 84.9 | 0.49 | 7.8 |

### decode

| image | bpp | implementation | n | min ms | median ms | p90 ms | max ms | median / NBIS | median extra RSS MiB |
|---|---|---|---|---|---|---|---|---|---|
| dark_ridges_5500x8000 | 0.75 | NBIS 5.0.0 | 11 | 3388.7 | 3682.8 | 3833.6 | 3900.2 | 1.00 | 335.8 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 1 thread | 11 | 1046.6 | 1197.1 | 1281.7 | 1285.8 | 0.33 | 352.1 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 2 threads | 11 | 659.4 | 828.3 | 1151.6 | 1180.4 | 0.22 | 355.2 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 4 threads | 11 | 615.7 | 780.0 | 1127.0 | 1271.5 | 0.21 | 360.8 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 8 threads | 11 | 510.5 | 734.7 | 912.9 | 972.7 | 0.20 | 372.4 |
| dark_ridges_5500x8000 | 2.25 | NBIS 5.0.0 | 11 | 4169.5 | 4339.1 | 4656.3 | 4793.8 | 1.00 | 335.8 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 1 thread | 11 | 1423.0 | 1542.5 | 1699.4 | 1746.5 | 0.36 | 352.1 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 2 threads | 11 | 889.0 | 998.7 | 1131.3 | 1263.5 | 0.23 | 355.2 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 4 threads | 11 | 775.2 | 926.1 | 1064.7 | 1320.5 | 0.21 | 360.8 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 8 threads | 11 | 689.0 | 907.0 | 1183.9 | 1460.0 | 0.21 | 372.3 |
| noise_1600x1500 | 0.75 | NBIS 5.0.0 | 31 | 143.5 | 178.0 | 226.6 | 283.0 | 1.00 | 18.5 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 47.0 | 56.2 | 81.2 | 118.1 | 0.32 | 22.8 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 2 threads | 31 | 49.0 | 56.0 | 77.5 | 92.1 | 0.31 | 23.6 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 44.3 | 56.4 | 72.3 | 84.9 | 0.32 | 25.3 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 8 threads | 31 | 44.9 | 57.8 | 70.7 | 80.3 | 0.32 | 28.8 |
| noise_1600x1500 | 2.25 | NBIS 5.0.0 | 31 | 184.0 | 227.8 | 279.8 | 326.8 | 1.00 | 18.5 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 63.5 | 78.4 | 103.8 | 122.2 | 0.34 | 22.8 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 2 threads | 31 | 66.1 | 80.5 | 108.8 | 117.6 | 0.35 | 23.6 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 60.3 | 80.6 | 99.8 | 135.8 | 0.35 | 25.5 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 8 threads | 31 | 59.9 | 74.8 | 94.7 | 139.7 | 0.33 | 28.8 |
| ridges_1600x1500 | 0.75 | NBIS 5.0.0 | 31 | 141.2 | 218.6 | 288.6 | 325.7 | 1.00 | 18.5 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 47.2 | 60.4 | 87.3 | 149.0 | 0.28 | 22.6 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 2 threads | 31 | 48.7 | 59.3 | 98.6 | 120.4 | 0.27 | 23.6 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 47.7 | 69.2 | 92.7 | 116.4 | 0.32 | 25.5 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 8 threads | 31 | 48.4 | 70.9 | 85.2 | 108.8 | 0.32 | 28.9 |
| ridges_1600x1500 | 2.25 | NBIS 5.0.0 | 31 | 185.7 | 263.3 | 367.2 | 431.8 | 1.00 | 18.5 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 67.0 | 81.3 | 132.5 | 146.7 | 0.31 | 22.6 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 2 threads | 31 | 71.2 | 95.4 | 120.6 | 148.3 | 0.36 | 23.6 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 71.4 | 98.4 | 139.2 | 188.7 | 0.37 | 25.5 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 8 threads | 31 | 59.3 | 87.0 | 146.0 | 174.2 | 0.33 | 28.9 |
| ridges_800x750 | 0.75 | NBIS 5.0.0 | 31 | 31.5 | 69.7 | 110.2 | 145.0 | 1.00 | 4.8 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 12.2 | 24.1 | 50.9 | 63.8 | 0.35 | 6.8 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 2 threads | 31 | 13.9 | 27.6 | 38.1 | 59.7 | 0.40 | 7.4 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 15.8 | 33.6 | 48.5 | 87.3 | 0.48 | 8.3 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 8 threads | 31 | 19.0 | 30.0 | 60.4 | 83.1 | 0.43 | 10.0 |
| ridges_800x750 | 2.25 | NBIS 5.0.0 | 31 | 45.5 | 85.6 | 151.3 | 180.8 | 1.00 | 4.8 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 15.7 | 25.5 | 47.8 | 59.5 | 0.30 | 6.9 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 2 threads | 31 | 18.8 | 34.8 | 56.3 | 74.2 | 0.41 | 7.5 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 19.7 | 39.1 | 61.9 | 102.5 | 0.46 | 8.2 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 8 threads | 31 | 20.5 | 35.3 | 59.3 | 72.7 | 0.41 | 10.0 |

