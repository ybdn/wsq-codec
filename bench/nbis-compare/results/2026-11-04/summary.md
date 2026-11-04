Output size and hash identical across every timed call and implementation: yes

### encode

| image | bpp | implementation | n | min ms | median ms | p90 ms | max ms | median / NBIS | median extra RSS MiB |
|---|---|---|---|---|---|---|---|---|---|
| dark_ridges_5500x8000 | 0.75 | NBIS 5.0.0 | 11 | 3492.1 | 3926.2 | 4556.9 | 4870.1 | 1.00 | 335.6 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 1 thread | 11 | 1281.3 | 1393.1 | 1678.7 | 1774.6 | 0.35 | 339.7 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 4 threads | 11 | 783.8 | 1658.5 | 1996.5 | 2014.2 | 0.42 | 348.5 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (crates.io release), 1 thread | 11 | 1209.0 | 1325.6 | 1554.3 | 1951.3 | 0.34 | 339.6 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (crates.io release), 4 threads | 11 | 599.8 | 1544.7 | 1720.4 | 2120.0 | 0.39 | 344.4 |
| dark_ridges_5500x8000 | 2.25 | NBIS 5.0.0 | 11 | 4006.1 | 4307.1 | 5167.6 | 5182.7 | 1.00 | 335.6 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 1 thread | 11 | 1648.6 | 1739.5 | 1901.0 | 2027.0 | 0.40 | 339.7 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 4 threads | 11 | 1052.5 | 1583.6 | 2175.6 | 2352.9 | 0.37 | 348.5 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (crates.io release), 1 thread | 11 | 1617.9 | 1732.7 | 1950.0 | 2371.6 | 0.40 | 339.7 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (crates.io release), 4 threads | 11 | 760.5 | 1373.0 | 2005.5 | 2163.9 | 0.32 | 344.9 |
| noise_1600x1500 | 0.75 | NBIS 5.0.0 | 31 | 109.0 | 120.9 | 158.9 | 206.8 | 1.00 | 18.2 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 50.4 | 58.0 | 87.7 | 109.9 | 0.48 | 19.5 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 40.1 | 57.2 | 98.6 | 99.2 | 0.47 | 21.9 |
| noise_1600x1500 | 0.75 | wsq-codec (crates.io release), 1 thread | 31 | 48.3 | 55.7 | 82.5 | 118.4 | 0.46 | 19.4 |
| noise_1600x1500 | 0.75 | wsq-codec (crates.io release), 4 threads | 31 | 33.9 | 45.8 | 85.8 | 110.9 | 0.38 | 20.6 |
| noise_1600x1500 | 2.25 | NBIS 5.0.0 | 31 | 136.1 | 154.5 | 189.1 | 236.7 | 1.00 | 18.2 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 71.3 | 82.0 | 106.3 | 160.6 | 0.53 | 19.5 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 51.4 | 68.8 | 111.8 | 154.3 | 0.44 | 21.4 |
| noise_1600x1500 | 2.25 | wsq-codec (crates.io release), 1 thread | 31 | 67.7 | 76.9 | 112.0 | 199.9 | 0.50 | 19.5 |
| noise_1600x1500 | 2.25 | wsq-codec (crates.io release), 4 threads | 31 | 43.9 | 61.3 | 113.6 | 150.8 | 0.40 | 20.6 |
| ridges_1600x1500 | 0.75 | NBIS 5.0.0 | 31 | 105.7 | 138.8 | 194.4 | 251.6 | 1.00 | 18.2 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 48.4 | 61.9 | 86.0 | 183.4 | 0.45 | 19.5 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 39.6 | 91.5 | 119.5 | 174.2 | 0.66 | 21.3 |
| ridges_1600x1500 | 0.75 | wsq-codec (crates.io release), 1 thread | 31 | 47.3 | 65.2 | 104.6 | 132.9 | 0.47 | 19.5 |
| ridges_1600x1500 | 0.75 | wsq-codec (crates.io release), 4 threads | 31 | 35.3 | 87.6 | 115.3 | 138.2 | 0.63 | 20.3 |
| ridges_1600x1500 | 2.25 | NBIS 5.0.0 | 31 | 135.2 | 163.2 | 214.1 | 234.3 | 1.00 | 18.2 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 71.8 | 85.8 | 101.2 | 190.9 | 0.53 | 19.5 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 52.6 | 106.7 | 141.4 | 167.9 | 0.65 | 21.2 |
| ridges_1600x1500 | 2.25 | wsq-codec (crates.io release), 1 thread | 31 | 64.6 | 80.3 | 126.9 | 175.2 | 0.49 | 19.4 |
| ridges_1600x1500 | 2.25 | wsq-codec (crates.io release), 4 threads | 31 | 45.7 | 98.4 | 128.0 | 172.6 | 0.60 | 20.3 |
| ridges_800x750 | 0.75 | NBIS 5.0.0 | 31 | 21.7 | 26.4 | 39.2 | 58.4 | 1.00 | 4.5 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 11.7 | 13.1 | 20.7 | 23.0 | 0.50 | 5.2 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 12.6 | 28.7 | 43.9 | 48.4 | 1.09 | 5.9 |
| ridges_800x750 | 0.75 | wsq-codec (crates.io release), 1 thread | 31 | 11.0 | 13.1 | 17.3 | 28.7 | 0.50 | 5.2 |
| ridges_800x750 | 0.75 | wsq-codec (crates.io release), 4 threads | 31 | 11.3 | 28.1 | 40.6 | 48.6 | 1.06 | 5.6 |
| ridges_800x750 | 2.25 | NBIS 5.0.0 | 31 | 28.0 | 36.6 | 48.1 | 110.6 | 1.00 | 4.5 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 17.1 | 19.6 | 27.7 | 30.1 | 0.54 | 5.2 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 17.8 | 29.8 | 53.4 | 193.3 | 0.82 | 5.9 |
| ridges_800x750 | 2.25 | wsq-codec (crates.io release), 1 thread | 31 | 16.3 | 20.6 | 28.6 | 34.8 | 0.56 | 5.2 |
| ridges_800x750 | 2.25 | wsq-codec (crates.io release), 4 threads | 31 | 17.5 | 29.0 | 50.4 | 84.8 | 0.79 | 5.6 |

### decode

| image | bpp | implementation | n | min ms | median ms | p90 ms | max ms | median / NBIS | median extra RSS MiB |
|---|---|---|---|---|---|---|---|---|---|
| dark_ridges_5500x8000 | 0.75 | NBIS 5.0.0 | 11 | 4026.5 | 4704.4 | 5218.7 | 5783.8 | 1.00 | 335.8 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 1 thread | 11 | 1191.2 | 1290.2 | 1567.1 | 1674.6 | 0.27 | 341.6 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (this checkout), 4 threads | 11 | 799.9 | 1322.4 | 1790.8 | 1813.2 | 0.28 | 350.7 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (crates.io release), 1 thread | 11 | 1133.3 | 1199.4 | 1545.3 | 1774.1 | 0.25 | 341.5 |
| dark_ridges_5500x8000 | 0.75 | wsq-codec (crates.io release), 4 threads | 11 | 845.2 | 1295.7 | 1854.2 | 1856.9 | 0.28 | 343.2 |
| dark_ridges_5500x8000 | 2.25 | NBIS 5.0.0 | 11 | 4888.8 | 5504.1 | 5910.7 | 6172.4 | 1.00 | 335.8 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 1 thread | 11 | 1359.3 | 1487.2 | 1657.0 | 1710.5 | 0.27 | 341.6 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (this checkout), 4 threads | 11 | 838.4 | 1687.7 | 1922.4 | 1944.0 | 0.31 | 350.6 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (crates.io release), 1 thread | 11 | 1301.5 | 1434.7 | 1778.3 | 1792.2 | 0.26 | 341.5 |
| dark_ridges_5500x8000 | 2.25 | wsq-codec (crates.io release), 4 threads | 11 | 811.0 | 1625.1 | 1983.8 | 2350.0 | 0.30 | 344.8 |
| noise_1600x1500 | 0.75 | NBIS 5.0.0 | 31 | 144.3 | 157.1 | 226.5 | 312.5 | 1.00 | 18.5 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 44.4 | 51.3 | 71.2 | 77.8 | 0.33 | 20.0 |
| noise_1600x1500 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 33.1 | 50.8 | 75.7 | 90.7 | 0.32 | 22.5 |
| noise_1600x1500 | 0.75 | wsq-codec (crates.io release), 1 thread | 31 | 43.2 | 50.6 | 73.0 | 82.4 | 0.32 | 19.9 |
| noise_1600x1500 | 0.75 | wsq-codec (crates.io release), 4 threads | 31 | 31.2 | 46.3 | 87.7 | 116.2 | 0.29 | 22.2 |
| noise_1600x1500 | 2.25 | NBIS 5.0.0 | 31 | 179.5 | 204.0 | 295.4 | 324.4 | 1.00 | 18.5 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 52.4 | 61.4 | 73.6 | 89.7 | 0.30 | 20.0 |
| noise_1600x1500 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 44.2 | 68.2 | 94.0 | 115.6 | 0.33 | 22.4 |
| noise_1600x1500 | 2.25 | wsq-codec (crates.io release), 1 thread | 31 | 51.6 | 62.1 | 113.6 | 156.1 | 0.30 | 19.9 |
| noise_1600x1500 | 2.25 | wsq-codec (crates.io release), 4 threads | 31 | 44.5 | 57.6 | 99.2 | 163.0 | 0.28 | 22.2 |
| ridges_1600x1500 | 0.75 | NBIS 5.0.0 | 31 | 139.0 | 202.0 | 268.0 | 313.0 | 1.00 | 18.5 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 46.1 | 52.8 | 98.6 | 147.1 | 0.26 | 20.0 |
| ridges_1600x1500 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 37.1 | 80.1 | 94.7 | 123.8 | 0.40 | 21.9 |
| ridges_1600x1500 | 0.75 | wsq-codec (crates.io release), 1 thread | 31 | 42.9 | 61.4 | 122.6 | 159.2 | 0.30 | 19.9 |
| ridges_1600x1500 | 0.75 | wsq-codec (crates.io release), 4 threads | 31 | 34.1 | 75.8 | 135.2 | 177.5 | 0.38 | 21.7 |
| ridges_1600x1500 | 2.25 | NBIS 5.0.0 | 31 | 182.3 | 241.9 | 317.0 | 403.4 | 1.00 | 18.5 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 55.9 | 68.1 | 84.4 | 139.6 | 0.28 | 20.0 |
| ridges_1600x1500 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 47.8 | 94.5 | 113.4 | 164.3 | 0.39 | 21.8 |
| ridges_1600x1500 | 2.25 | wsq-codec (crates.io release), 1 thread | 31 | 54.8 | 84.8 | 136.9 | 176.4 | 0.35 | 19.9 |
| ridges_1600x1500 | 2.25 | wsq-codec (crates.io release), 4 threads | 31 | 50.1 | 96.6 | 151.6 | 165.2 | 0.40 | 21.8 |
| ridges_800x750 | 0.75 | NBIS 5.0.0 | 31 | 30.2 | 40.4 | 56.7 | 111.4 | 1.00 | 4.8 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 1 thread | 31 | 10.7 | 12.8 | 23.2 | 26.5 | 0.32 | 5.4 |
| ridges_800x750 | 0.75 | wsq-codec (this checkout), 4 threads | 31 | 11.0 | 22.9 | 43.1 | 51.4 | 0.57 | 6.2 |
| ridges_800x750 | 0.75 | wsq-codec (crates.io release), 1 thread | 31 | 10.3 | 12.1 | 19.2 | 23.8 | 0.30 | 5.4 |
| ridges_800x750 | 0.75 | wsq-codec (crates.io release), 4 threads | 31 | 10.5 | 27.9 | 42.5 | 47.4 | 0.69 | 6.1 |
| ridges_800x750 | 2.25 | NBIS 5.0.0 | 31 | 39.9 | 54.0 | 70.5 | 84.1 | 1.00 | 4.8 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 1 thread | 31 | 13.5 | 15.7 | 22.9 | 36.0 | 0.29 | 5.4 |
| ridges_800x750 | 2.25 | wsq-codec (this checkout), 4 threads | 31 | 14.6 | 27.3 | 47.3 | 67.0 | 0.51 | 6.2 |
| ridges_800x750 | 2.25 | wsq-codec (crates.io release), 1 thread | 31 | 13.1 | 16.8 | 25.6 | 31.7 | 0.31 | 5.4 |
| ridges_800x750 | 2.25 | wsq-codec (crates.io release), 4 threads | 31 | 12.8 | 26.3 | 45.3 | 55.1 | 0.49 | 6.0 |

