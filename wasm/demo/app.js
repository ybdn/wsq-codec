// The demo page. Everything happens here, in the browser: files are read with the File API,
// decoded and encoded by the WebAssembly module, drawn on canvases, and offered back as blob:
// downloads. No file is ever passed to fetch, XMLHttpRequest, WebSocket or any other API that
// could send it, and the page's Content-Security-Policy (see index.html) forbids every
// connection but to its own origin. Text that comes from a file is shown with textContent only.

import init, { decode, encode, version } from "./wsq/wsq_codec.js";

const $ = (id) => document.getElementById(id);

// A blob: URL for each download, revoked when the next result replaces it.
let downloadUrl = null;
// What the "Download PNG" button saves.
let decodedCanvas = null;
// The grayscale image waiting to be encoded.
let source = null;

function setStatus(id, message, isError = false) {
  const el = $(id);
  el.textContent = message;
  el.classList.toggle("error", isError);
}

function showInfo(dl, rows) {
  dl.replaceChildren();
  for (const [label, value, preformatted] of rows) {
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    if (preformatted) {
      const pre = document.createElement("pre");
      pre.textContent = value;
      dd.append(pre);
    } else {
      dd.textContent = value;
    }
    dl.append(dt, dd);
  }
}

const errorText = (e) => (e && e.message ? e.message : String(e));
const nextFrame = () => new Promise((resolve) => setTimeout(resolve, 30));
const formatBytes = (n) => `${n.toLocaleString("en-US")} bytes`;
const baseName = (name) => name.replace(/\.[^.]*$/, "").replace(/[^\w.-]+/g, "_") || "image";

/** Draws 8-bit grayscale pixels on a canvas. */
function drawGray(canvas, pixels, width, height) {
  canvas.width = width;
  canvas.height = height;
  const rgba = new Uint8ClampedArray(width * height * 4);
  for (let i = 0, j = 0; i < pixels.length; i++, j += 4) {
    rgba[j] = rgba[j + 1] = rgba[j + 2] = pixels[i];
    rgba[j + 3] = 255;
  }
  canvas.getContext("2d").putImageData(new ImageData(rgba, width, height), 0, 0);
}

/** Decodes the browser's own image formats to grayscale, with transparency over white. */
async function grayscaleFromBlob(blob) {
  const bitmap = await createImageBitmap(blob);
  try {
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    ctx.drawImage(bitmap, 0, 0);
    const { data } = ctx.getImageData(0, 0, bitmap.width, bitmap.height);
    const pixels = new Uint8Array(bitmap.width * bitmap.height);
    for (let i = 0, j = 0; i < pixels.length; i++, j += 4) {
      const luma = 0.299 * data[j] + 0.587 * data[j + 1] + 0.114 * data[j + 2];
      const alpha = data[j + 3] / 255;
      pixels[i] = Math.round(luma * alpha + 255 * (1 - alpha));
    }
    return { pixels, width: bitmap.width, height: bitmap.height };
  } finally {
    bitmap.close();
  }
}

// --- WSQ to image -----------------------------------------------------------------------

async function openWsq(bytes, name) {
  $("decode-result").hidden = true;
  setStatus("decode-status", `Decoding ${name}…`);
  await nextFrame();
  try {
    const start = performance.now();
    const image = decode(bytes);
    const elapsed = performance.now() - start;
    const { width, height } = image;
    const pixels = image.pixels;
    drawGray($("decode-canvas"), pixels, width, height);
    decodedCanvas = $("decode-canvas");
    decodedCanvas.dataset.name = baseName(name);

    const comments = image.comments;
    showInfo($("decode-info"), [
      ["File", `${name}, ${formatBytes(bytes.length)}`],
      ["Size", `${width} × ${height} pixels`],
      ["Resolution", image.ppi === undefined ? "not recorded" : `${image.ppi} ppi`],
      ["Implementation number", String(image.software)],
      ["Compression", `${(bytes.length * 8 / (width * height)).toFixed(3)} bits per pixel, ${(width * height / bytes.length).toFixed(1)}:1`],
      ["Decoded in", `${elapsed.toFixed(0)} ms`],
      ["Comments", comments.length ? comments.join("\n\n") : "none", comments.length > 0],
    ]);
    image.free();
    setStatus("decode-status", "");
    $("decode-result").hidden = false;
  } catch (e) {
    setStatus("decode-status", `Cannot decode ${name}: ${errorText(e)}`, true);
  }
}

async function openWsqFile(file) {
  openWsq(new Uint8Array(await file.arrayBuffer()), file.name);
}

$("wsq-file").addEventListener("change", (event) => {
  const file = event.target.files[0];
  if (file) openWsqFile(file);
  event.target.value = "";
});

$("wsq-sample").addEventListener("click", async () => {
  try {
    const response = await fetch("sample.wsq");
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    openWsq(new Uint8Array(await response.arrayBuffer()), "sample.wsq");
  } catch (e) {
    setStatus("decode-status", `Cannot load the sample: ${errorText(e)}`, true);
  }
});

$("save-png").addEventListener("click", () => {
  if (!decodedCanvas) return;
  decodedCanvas.toBlob((blob) => {
    if (blob) saveBlob(blob, `${decodedCanvas.dataset.name || "image"}.png`);
  }, "image/png");
});

/** Offers a blob as a download, through a temporary link: the data stays in the page. */
function saveBlob(blob, filename) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

// --- Image to WSQ -----------------------------------------------------------------------

async function openImage(blob, name) {
  $("encode-result").hidden = true;
  $("encode-form").hidden = true;
  setStatus("encode-status", `Reading ${name}…`);
  await nextFrame();
  try {
    source = { ...(await grayscaleFromBlob(blob)), name: baseName(name) };
    drawGray($("source-canvas"), source.pixels, source.width, source.height);
    $("source-info").textContent = `${name}: ${source.width} × ${source.height} pixels, as grayscale.`;
    $("encode-form").hidden = false;
    setStatus("encode-status", "");
  } catch (e) {
    source = null;
    setStatus("encode-status", `Cannot read ${name} as an image: ${errorText(e)}`, true);
  }
}

$("image-file").addEventListener("change", (event) => {
  const file = event.target.files[0];
  if (file) openImage(file, file.name);
  event.target.value = "";
});

$("image-sample").addEventListener("click", async () => {
  try {
    const response = await fetch("sample.png");
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    openImage(await response.blob(), "sample.png");
  } catch (e) {
    setStatus("encode-status", `Cannot load the sample: ${errorText(e)}`, true);
  }
});

/** Peak signal-to-noise ratio between two 8-bit images, in dB. */
function psnr(a, b) {
  let sum = 0;
  for (let i = 0; i < a.length; i++) {
    const d = a[i] - b[i];
    sum += d * d;
  }
  return sum === 0 ? Infinity : 10 * Math.log10((255 * 255) / (sum / a.length));
}

$("encode-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  if (!source) return;
  const button = $("encode-button");
  button.disabled = true;
  $("encode-result").hidden = true;
  setStatus("encode-status", "Encoding…");
  await nextFrame();
  try {
    const options = {
      bitrate: Number($("bitrate").value),
      profile: $("profile").value,
    };
    if ($("ppi").value.trim() !== "") options.ppi = Number($("ppi").value);
    if ($("comment").value !== "") options.comment = $("comment").value;

    const start = performance.now();
    const wsq = encode(source.pixels, source.width, source.height, options);
    const elapsed = performance.now() - start;

    // Decode it again, to show what the compression did.
    const back = decode(wsq);
    const quality = psnr(source.pixels, back.pixels);
    drawGray($("roundtrip-canvas"), back.pixels, back.width, back.height);
    back.free();

    const raw = source.width * source.height;
    showInfo($("encode-info"), [
      ["WSQ size", `${formatBytes(wsq.length)} (from ${formatBytes(raw)} of pixels)`],
      ["Compression", `${(wsq.length * 8 / raw).toFixed(3)} bits per pixel, ${(raw / wsq.length).toFixed(1)}:1`],
      ["Quality", Number.isFinite(quality) ? `${quality.toFixed(1)} dB PSNR` : "lossless"],
      ["Profile", options.profile],
      ["Encoded in", `${elapsed.toFixed(0)} ms`],
    ]);

    if (downloadUrl) URL.revokeObjectURL(downloadUrl);
    downloadUrl = URL.createObjectURL(new Blob([wsq], { type: "application/octet-stream" }));
    const link = $("download");
    link.href = downloadUrl;
    link.download = `${source.name}.wsq`;
    setStatus("encode-status", "");
    $("encode-result").hidden = false;
  } catch (e) {
    setStatus("encode-status", `Cannot encode: ${errorText(e)}`, true);
  } finally {
    button.disabled = false;
  }
});

// --- Drag and drop ----------------------------------------------------------------------

function dropTarget(card, handler) {
  card.addEventListener("dragover", (event) => {
    event.preventDefault();
    card.classList.add("dragging");
  });
  card.addEventListener("dragleave", () => card.classList.remove("dragging"));
  card.addEventListener("drop", (event) => {
    event.preventDefault();
    card.classList.remove("dragging");
    const file = event.dataTransfer.files[0];
    if (file && !$("wsq-file").disabled) handler(file);
  });
}

dropTarget($("decode-card"), openWsqFile);
dropTarget($("encode-card"), (file) => openImage(file, file.name));

// A file dropped outside the cards must not make the browser navigate to it.
for (const type of ["dragover", "drop"]) {
  window.addEventListener(type, (event) => event.preventDefault());
}

// --- Start ------------------------------------------------------------------------------

try {
  await init();
  $("version").textContent = `v${version()}`;
  $("loading").hidden = true;
  for (const id of ["wsq-file", "wsq-sample", "image-file", "image-sample"]) {
    $(id).disabled = false;
  }
} catch (e) {
  $("loading").hidden = true;
  const fatal = $("fatal");
  fatal.textContent = `The codec could not be loaded: ${errorText(e)}. This page needs a browser with WebAssembly.`;
  fatal.hidden = false;
}

// Offline use: a service worker keeps the page's own files (never a user's file).
if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("sw.js").catch(() => {});
}
