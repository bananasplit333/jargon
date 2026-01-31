import React from "react";
import ReactDOM from "react-dom/client";
import './style.css';
import { AppLayout } from "./layout/AppLayout";
import { BrowserRouter } from "react-router-dom";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <BrowserRouter>
      <AppLayout />
    </BrowserRouter>
  </React.StrictMode>,
);
