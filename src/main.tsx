import React from "react";
import ReactDOM from "react-dom/client";
import { ThemeProvider } from "styled-components";
import { App } from "./App";
import { ChakraProvider, ColorModeScript } from "@chakra-ui/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { SettingsProvider } from "./Providers/SettingsProvider";
import { RecordingStateProvider } from "./Providers/RecordingStateProvider";
import { theme } from "./theme";
import { attachConsole } from "tauri-plugin-log-api";
import ChromeAutomationApp from "./ChromeAutomationApp";
import { BrowserRouter } from "react-router-dom";
import "@heelix-app/design/index.css";

// Use a query parameter to determine which app to render
const urlParams = new URLSearchParams(window.location.search);
const showChromeAutomation = urlParams.get('chromeAutomation') === 'true';

const queryClient = new QueryClient();

attachConsole();

if (showChromeAutomation) {
  // Render the standalone Chrome Automation app
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <ChromeAutomationApp />
    </React.StrictMode>
  );
} else {
  // Render the regular app
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <BrowserRouter>
        <QueryClientProvider client={queryClient}>
          <ThemeProvider theme={theme}>
            <ChakraProvider theme={theme}>
              <ColorModeScript initialColorMode={theme.config.initialColorMode} />
              <SettingsProvider>
                <RecordingStateProvider>
                  <App />
                </RecordingStateProvider>
              </SettingsProvider>
            </ChakraProvider>
          </ThemeProvider>
        </QueryClientProvider>
      </BrowserRouter>
    </React.StrictMode>
  );
}
