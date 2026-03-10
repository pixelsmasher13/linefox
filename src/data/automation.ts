import { invoke } from "@tauri-apps/api/core";
import { Automation, AutomationScript } from "../screens/AgentScreen/types";

export const fetchAutomations = async (): Promise<Automation[]> => {
  try {
    return await invoke<Automation[]>("get_all_automations");
  } catch (error) {
    console.error("Error fetching automations:", error);
    return [];
  }
};

export const fetchAutomationHistory = async (): Promise<Automation[]> => {
  try {
    return await invoke<Automation[]>("get_automation_history");
  } catch (error) {
    console.error("Error fetching automation history:", error);
    return [];
  }
};

export const fetchAutomationById = async (automationId: number): Promise<Automation | null> => {
  try {
    return await invoke<Automation>("get_automation_by_id", { automationId });
  } catch (error) {
    console.error("Error fetching automation by id:", error);
    return null;
  }
};

export const fetchAutomationScript = async (automationId: number): Promise<AutomationScript> => {
  return await invoke<AutomationScript>("get_automation_script", { automationId });
};

export const startRecordingAutomation = async (objective: string, name: string): Promise<void> => {
  await invoke("start_recording_automation", { objective, name });
};

export const stopRecordingAutomation = async (): Promise<void> => {
  await invoke("stop_recording_automation");
};

export const playAutomation = async (automationId: number, additionalInstructions?: string, agentMode?: boolean): Promise<void> => {
  await invoke("play_automation", {
    automationId,
    additionalInstructions: additionalInstructions || "",
    agentMode: agentMode ?? false
  });
};

export const stopAutomationPlayback = async (): Promise<void> => {
  await invoke("stop_automation");
};

export const updateAutomationScript = async (automationId: number, script: AutomationScript): Promise<void> => {
  const { name, nl_description, objective, steps, additional_instructions, created_at, updated_at } = script;
  
  await invoke("update_automation_script", { 
    automationId,
    script: {
      id: automationId,
      name,
      nl_description, 
      objective,
      steps,
      additional_instructions,
      created_at,
      updated_at
    }
  });
};

export const deleteAutomation = async (automationId: number): Promise<void> => {
  await invoke("delete_automation", { automationId });
};

export const automationService = {
  fetchAutomations,
  fetchAutomationHistory,
  fetchAutomationById,
  fetchAutomationScript,
  startRecordingAutomation,
  stopRecordingAutomation,
  playAutomation,
  stopAutomationPlayback,
  updateAutomationScript,
  deleteAutomation
}; 