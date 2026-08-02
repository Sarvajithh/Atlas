/** Image Viewer (§8.2.4). Renders real base64 bytes from `document.read`. */
export function ImageViewer({ base64, mime, zoom }: { base64: string; mime: string; zoom: number }) {
  return (
    <div className="flex h-full items-center justify-center overflow-auto bg-muted/30 p-4">
      <img
        src={`data:${mime};base64,${base64}`}
        alt=""
        style={{ transform: `scale(${zoom})`, transformOrigin: "center" }}
        className="max-w-none"
      />
    </div>
  );
}
