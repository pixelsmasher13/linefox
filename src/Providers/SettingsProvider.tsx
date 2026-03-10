import {
  createContext,
  useContext,
  type FC,
  type PropsWithChildren,
  useState,
  useEffect,
  useRef,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { enable, disable, isEnabled } from "tauri-plugin-autostart-api";
import {
  type SettingDbItem,
  settingDbItemsZod,
} from "./RecordingStateProvider";

export const DEFAULT_SETTINGS: Settings = {
  is_dev_mode: false,
  interval: "10",
  auto_start: false,
  api_choice: "claude",
  api_key_claude: "",
  api_key_open_ai: "",
  api_key_grok: "",
  api_key_gemini: "",
  api_key_deepseek: "",
  use_pro_model: false,
  store_task_data: true,
  model_claude: "claude-sonnet-4-5-20250929",
  model_openai: "gpt-5",
  model_grok: "grok-3",
  model_gemini: "gemini-2.5-flash",
  model_deepseek: "deepseek-chat",
};

type Update = {
  (settings: Settings): Promise<void>;
};

type ApiChoice = "claude" | "openai" | "grok" | "gemini" | "deepseek";
export type Settings = {
  is_dev_mode: boolean;
  interval: string;
  auto_start: boolean;
  api_choice: ApiChoice;
  api_key_claude: string;
  api_key_open_ai: string;
  api_key_grok: string;
  api_key_gemini: string;
  api_key_deepseek: string;
  use_pro_model: boolean;
  store_task_data: boolean;
  model_claude: string;
  model_openai: string;
  model_grok: string;
  model_gemini: string;
  model_deepseek: string;
};

type SettingsContextType = {
  settings: Settings;
  update: Update;
  refresh: () => Promise<void>;
};

const SettingsContext = createContext<SettingsContextType | undefined>(
  undefined
);

export const SettingsProvider: FC<PropsWithChildren> = ({ children }) => {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);

  const getSettingOrEmpty = (
    settings: SettingDbItem[],
    settingKey: string
  ): string => {
    const filtered = settings
      .filter((setting) => setting.setting_key == settingKey)
      .map((setting) => setting.setting_value);
    if (filtered != null && filtered.length > 0) {
      return filtered[0];
    }
    return "";
  };

  const buildSettings = (response: SettingDbItem[]): Settings => {
    // For store_task_data, default to true if not set
    const storeTaskDataValue = getSettingOrEmpty(response, "store_task_data");
    const storeTaskData = storeTaskDataValue === "" ? true : storeTaskDataValue !== "false";
    
    return {
      interval: getSettingOrEmpty(response, "interval") || "20",
      is_dev_mode: getSettingOrEmpty(response, "is_dev_mode") == "true",
      auto_start: getSettingOrEmpty(response, "auto_start") == "true",
      api_choice:
        (getSettingOrEmpty(response, "api_choice") as ApiChoice) || "claude",
      api_key_claude: getSettingOrEmpty(response, "api_key_claude") || "",
      api_key_open_ai: getSettingOrEmpty(response, "api_key_open_ai") || "",
      api_key_grok: getSettingOrEmpty(response, "api_key_grok") || "",
      api_key_gemini: getSettingOrEmpty(response, "api_key_gemini") || "",
      api_key_deepseek: getSettingOrEmpty(response, "api_key_deepseek") || "",
      use_pro_model: getSettingOrEmpty(response, "use_pro_model") === "true",
      store_task_data: storeTaskData,
      model_claude: getSettingOrEmpty(response, "model_claude") || "claude-sonnet-4-5-20250929",
      model_openai: getSettingOrEmpty(response, "model_openai") || "gpt-5",
      model_grok: getSettingOrEmpty(response, "model_grok") || "grok-3",
      model_gemini: getSettingOrEmpty(response, "model_gemini") || "gemini-2.5-flash",
      model_deepseek: getSettingOrEmpty(response, "model_deepseek") || "deepseek-chat",
    };
  };

  const refresh = async () => {
    try {
      const response = await invoke("get_latest_settings");
      const parsed = settingDbItemsZod.safeParse(response);
      if (parsed.success) {
        const builtSettings = buildSettings(parsed.data);
        // Apply DB settings immediately
        setSettings(builtSettings);
        // Then update auto_start from OS state
        try {
          const autoStartEnabled = await isEnabled();
          setSettings((prev) => ({ ...prev, auto_start: autoStartEnabled }));
        } catch (e) {
          console.warn("Failed to check OS autostart state:", e);
        }
      } else {
        console.error("invoke get_latest_settings Error:", parsed.error);
      }
    } catch (err) {
      console.error("get_latest_settings invoke failed:", err);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const update: Update = async (newSettings) => {
    if (newSettings.auto_start !== settings.auto_start) {
      if (newSettings.auto_start) {
        await enable();
      } else {
        await disable();
      }
    }
    updateSettingsOnRust(newSettings);
    setSettings(newSettings);
    return Promise.resolve();
  };

  return (
    <SettingsContext.Provider value={{ settings, update, refresh }}>
      {children}
    </SettingsContext.Provider>
  );
};

const updateSettingsOnRust = (settings: Settings) => {
  invoke("update_settings", { settings }).then();
};

export const useGlobalSettings = (): SettingsContextType => {
  const context = useContext(SettingsContext);
  if (context === undefined) {
    throw Error("SettingsContext must be used within a SettingsProvider");
  }
  return context;
};
