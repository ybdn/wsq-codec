Output size and hash identical across every timed call and implementation: yes

### encode

| image | bpp | implementation | n | min ms | median ms | p90 ms | max ms | median / NBIS | median extra RSS MiB |
|---|---|---|---|---|---|---|---|---|---|
| dark_ridges_5500x8000 | 0.75 | NBIS 5.0.0 | 11 | 2642.0 | 2774.1 | 3012.2 | 3106.3 | 1.00 | 335.6 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 1 thread | 11 | 1190.1 | 1420.9 | 1595.3 | 1848.0 | 0.51 | 350.2 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 2 threads | 11 | 787.0 | 928.5 | 1397.9 | 1683.0 | 0.33 | 353.3 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 4 threads | 11 | 580.5 | 1054.5 | 1270.9 | 1407.3 | 0.38 | 359.1 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 8 threads | 11 | 644.9 | 923.9 | 1178.5 | 1313.6 | 0.33 | 370.4 |
| dark_ridges_5500x8000 | 2.25 | NBIS 5.0.0 | 11 | 3210.1 | 3351.0 | 3736.4 | 3904.0 | 1.00 | 335.6 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 1 thread | 11 | 1671.2 | 1856.6 | 1920.4 | 1980.8 | 0.55 | 350.3 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 2 threads | 11 | 1034.7 | 1235.2 | 1869.8 | 2249.7 | 0.37 | 353.3 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 4 threads | 11 | 841.0 | 1229.3 | 1812.8 | 1950.1 | 0.37 | 359.1 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 8 threads | 11 | 808.7 | 1016.8 | 1611.1 | 2178.6 | 0.30 | 370.5 |
| noise_1600x1500 | 0.75 | NBIS 5.0.0 | 31 | 106.4 | 138.2 | 175.5 | 179.5 | 1.00 | 18.2 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 54.4 | 62.9 | 87.4 | 89.1 | 0.46 | 22.1 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 2 threads | 31 | 39.4 | 50.1 | 54.9 | 57.8 | 0.36 | 23.0 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 36.6 | 47.2 | 53.3 | 55.7 | 0.34 | 23.9 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 8 threads | 31 | 39.9 | 50.2 | 54.0 | 58.9 | 0.36 | 25.6 |
| noise_1600x1500 | 2.25 | NBIS 5.0.0 | 31 | 134.6 | 157.0 | 187.3 | 212.7 | 1.00 | 18.2 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 76.9 | 89.1 | 114.3 | 119.2 | 0.57 | 22.1 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 2 threads | 31 | 51.7 | 58.3 | 70.0 | 75.7 | 0.37 | 23.1 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 52.6 | 59.7 | 71.0 | 79.5 | 0.38 | 23.9 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 8 threads | 31 | 48.5 | 60.7 | 71.9 | 73.8 | 0.39 | 25.6 |
| ridges_1600x1500 | 0.75 | NBIS 5.0.0 | 31 | 104.0 | 135.2 | 170.4 | 175.4 | 1.00 | 18.2 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 50.5 | 58.4 | 76.9 | 93.9 | 0.43 | 22.1 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 2 threads | 31 | 38.8 | 49.0 | 59.9 | 80.5 | 0.36 | 23.0 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 38.3 | 50.2 | 65.1 | 68.8 | 0.37 | 23.9 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 8 threads | 31 | 42.3 | 54.6 | 65.4 | 70.9 | 0.40 | 25.7 |
| ridges_1600x1500 | 2.25 | NBIS 5.0.0 | 31 | 131.3 | 152.6 | 209.3 | 212.8 | 1.00 | 18.2 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 75.1 | 88.5 | 111.4 | 119.4 | 0.58 | 22.1 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 2 threads | 31 | 50.9 | 65.8 | 79.6 | 87.6 | 0.43 | 23.1 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 52.8 | 69.4 | 86.2 | 96.2 | 0.45 | 23.9 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 8 threads | 31 | 54.3 | 70.7 | 86.9 | 93.1 | 0.46 | 24.9 |
| ridges_800x750 | 0.75 | NBIS 5.0.0 | 31 | 22.6 | 24.9 | 40.7 | 42.2 | 1.00 | 4.5 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 11.6 | 14.5 | 20.4 | 22.3 | 0.58 | 6.7 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 2 threads | 31 | 10.7 | 14.1 | 17.8 | 19.4 | 0.57 | 7.1 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 11.8 | 15.7 | 20.1 | 23.4 | 0.63 | 7.2 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 8 threads | 31 | 13.9 | 18.5 | 23.7 | 32.1 | 0.74 | 7.9 |
| ridges_800x750 | 2.25 | NBIS 5.0.0 | 31 | 29.8 | 35.4 | 50.6 | 53.3 | 1.00 | 4.5 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 17.6 | 20.5 | 27.7 | 42.4 | 0.58 | 6.7 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 2 threads | 31 | 16.5 | 20.8 | 25.5 | 27.9 | 0.59 | 7.1 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 17.3 | 24.1 | 27.5 | 31.0 | 0.68 | 7.2 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 8 threads | 31 | 19.1 | 23.7 | 31.3 | 34.7 | 0.67 | 7.8 |

### decode

| image | bpp | implementation | n | min ms | median ms | p90 ms | max ms | median / NBIS | median extra RSS MiB |
|---|---|---|---|---|---|---|---|---|---|
| dark_ridges_5500x8000 | 0.75 | NBIS 5.0.0 | 11 | 3497.5 | 3899.8 | 4158.6 | 4167.8 | 1.00 | 335.8 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 1 thread | 11 | 1152.7 | 1281.9 | 1358.9 | 1360.0 | 0.33 | 352.1 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 2 threads | 11 | 757.3 | 813.4 | 1224.4 | 1367.2 | 0.21 | 355.2 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 4 threads | 11 | 537.5 | 942.9 | 1077.8 | 1089.0 | 0.24 | 360.8 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 8 threads | 11 | 513.1 | 868.1 | 973.8 | 982.1 | 0.22 | 372.3 |
| dark_ridges_5500x8000 | 2.25 | NBIS 5.0.0 | 11 | 4324.2 | 4680.4 | 5111.6 | 5179.9 | 1.00 | 335.8 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 1 thread | 11 | 1458.4 | 1587.5 | 1721.4 | 1739.1 | 0.34 | 352.1 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 2 threads | 11 | 931.1 | 1276.8 | 1584.9 | 1778.2 | 0.27 | 355.1 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 4 threads | 11 | 617.2 | 886.9 | 1317.4 | 1350.5 | 0.19 | 360.8 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 8 threads | 11 | 602.0 | 1211.9 | 1515.0 | 1683.5 | 0.26 | 372.3 |
| noise_1600x1500 | 0.75 | NBIS 5.0.0 | 31 | 145.4 | 165.4 | 241.5 | 245.3 | 1.00 | 18.5 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 49.4 | 59.3 | 75.9 | 80.6 | 0.36 | 22.8 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 2 threads | 31 | 33.4 | 42.5 | 48.3 | 51.2 | 0.26 | 23.5 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 32.1 | 40.8 | 44.6 | 55.2 | 0.25 | 25.2 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 8 threads | 31 | 32.1 | 42.7 | 50.2 | 53.0 | 0.26 | 28.8 |
| noise_1600x1500 | 2.25 | NBIS 5.0.0 | 31 | 184.0 | 205.1 | 268.5 | 308.1 | 1.00 | 18.5 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 65.6 | 73.3 | 98.5 | 105.0 | 0.36 | 22.8 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 2 threads | 31 | 46.7 | 57.0 | 65.9 | 73.3 | 0.28 | 23.5 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 41.6 | 49.3 | 59.8 | 67.9 | 0.24 | 25.5 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 8 threads | 31 | 43.5 | 53.2 | 61.1 | 68.9 | 0.26 | 28.9 |
| ridges_1600x1500 | 0.75 | NBIS 5.0.0 | 31 | 142.4 | 167.0 | 210.6 | 240.4 | 1.00 | 18.5 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 47.8 | 55.4 | 75.2 | 84.6 | 0.33 | 22.6 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 2 threads | 31 | 34.5 | 44.8 | 52.3 | 57.0 | 0.27 | 23.5 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 32.7 | 45.2 | 57.1 | 61.1 | 0.27 | 25.5 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 8 threads | 31 | 37.2 | 45.9 | 63.4 | 69.6 | 0.28 | 28.9 |
| ridges_1600x1500 | 2.25 | NBIS 5.0.0 | 31 | 180.5 | 209.1 | 279.9 | 311.7 | 1.00 | 18.5 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 67.9 | 80.6 | 102.6 | 109.5 | 0.39 | 22.6 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 2 threads | 31 | 43.6 | 58.0 | 65.8 | 78.1 | 0.28 | 23.5 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 43.6 | 57.5 | 72.8 | 80.3 | 0.28 | 25.5 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 8 threads | 31 | 46.1 | 61.2 | 77.8 | 79.7 | 0.29 | 28.9 |
| ridges_800x750 | 0.75 | NBIS 5.0.0 | 31 | 32.2 | 36.7 | 54.3 | 57.5 | 1.00 | 4.8 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 11.5 | 13.8 | 18.3 | 19.9 | 0.38 | 6.9 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 2 threads | 31 | 11.0 | 14.0 | 17.7 | 25.5 | 0.38 | 7.4 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 11.6 | 14.6 | 19.1 | 23.7 | 0.40 | 8.3 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 8 threads | 31 | 13.1 | 19.0 | 22.9 | 27.0 | 0.52 | 10.0 |
| ridges_800x750 | 2.25 | NBIS 5.0.0 | 31 | 41.9 | 53.8 | 72.7 | 77.4 | 1.00 | 4.8 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 16.5 | 19.0 | 25.6 | 27.1 | 0.35 | 7.0 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 2 threads | 31 | 14.0 | 18.1 | 21.2 | 23.9 | 0.34 | 7.4 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 14.9 | 18.9 | 23.4 | 27.4 | 0.35 | 8.3 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 8 threads | 31 | 17.3 | 23.9 | 31.1 | 33.6 | 0.44 | 10.0 |

