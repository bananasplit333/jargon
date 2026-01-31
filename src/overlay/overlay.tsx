import React from "react";
import ReactDOM from "react-dom/client";

function OverlayApp() {
  return (
    <div
      style={{
        background: "#000000",
        userSelect: "none",
        pointerEvents: "auto",
        boxShadow: "0 2px 1px rgba(0,0,0,0.8)",
      }}
    />
  );
}

const root = document.getElementById("overlay-root");
if (root) {
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <OverlayApp />
    </React.StrictMode>
  );
}
