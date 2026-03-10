import { useAtom } from "jotai";
import { atomWithReducer } from "jotai/utils";
import { useEffect } from "react";
import { automationService } from "../data/automation";
import { Automation, AutomationScript } from "../screens/AgentScreen/types";
import { listen } from "@tauri-apps/api/event";

type AutomationState = {
  automations: Automation[];
  automationHistory: Automation[];
  selectedAutomationId: number | null;
  automationScript: AutomationScript | null;
  isLoadingScript: boolean;
  isPlaying: boolean;
  playbackProgress: number;
  isEditing: boolean;
  isRecording: boolean;
  isProcessingRecording: boolean;
};

type AutomationAction =
  | { type: "setAutomations"; payload: Automation[] }
  | { type: "setAutomationHistory"; payload: Automation[] }
  | { type: "selectAutomation"; payload: number | null }
  | { type: "setAutomationScript"; payload: AutomationScript | null }
  | { type: "setIsLoadingScript"; payload: boolean }
  | { type: "setIsPlaying"; payload: boolean }
  | { type: "setPlaybackProgress"; payload: number }
  | { type: "setIsEditing"; payload: boolean }
  | { type: "setIsRecording"; payload: boolean }
  | { type: "setIsProcessingRecording"; payload: boolean }
  | { type: "deleteAutomation"; payload: number };

// Reducer
const automationReducer = (prev: AutomationState, action: AutomationAction): AutomationState => {
  switch (action.type) {
    case "setAutomations":
      return {
        ...prev,
        automations: action.payload,
      };

    case "setAutomationHistory":
      return {
        ...prev,
        automationHistory: action.payload,
      };

    case "selectAutomation":
      return {
        ...prev,
        selectedAutomationId: action.payload,
        // Clear automationScript when selecting a different automation to prevent showing wrong data
        automationScript: action.payload === null || action.payload !== prev.selectedAutomationId ? null : prev.automationScript,
        isLoadingScript: action.payload !== null && action.payload !== prev.selectedAutomationId,
        isEditing: false,
      };

    case "setAutomationScript":
      return {
        ...prev,
        automationScript: action.payload,
        isLoadingScript: false,
      };

    case "setIsLoadingScript":
      return {
        ...prev,
        isLoadingScript: action.payload,
      };

    case "setIsPlaying":
      return {
        ...prev,
        isPlaying: action.payload,
      };

    case "setPlaybackProgress":
      return {
        ...prev,
        playbackProgress: action.payload,
      };

    case "setIsEditing":
      return {
        ...prev,
        isEditing: action.payload,
      };

    case "setIsRecording":
      console.log("🔄 State update: setIsRecording from", prev.isRecording, "to", action.payload);
      return {
        ...prev,
        isRecording: action.payload,
      };

    case "setIsProcessingRecording":
      console.log("🔄 State update: setIsProcessingRecording from", prev.isProcessingRecording, "to", action.payload);
      return {
        ...prev,
        isProcessingRecording: action.payload,
      };

    case "deleteAutomation":
      const updatedAutomations = prev.automations.filter(
        (automation) => automation.id !== action.payload
      );
      
      // Reset selected automation if it was deleted
      const newSelectedId = 
        prev.selectedAutomationId === action.payload
          ? null
          : prev.selectedAutomationId;
          
      const newScript = 
        prev.selectedAutomationId === action.payload
          ? null
          : prev.automationScript;
          
      return {
        ...prev,
        automations: updatedAutomations,
        selectedAutomationId: newSelectedId,
        automationScript: newScript,
      };

    default:
      return prev;
  }
};

// Initial state
const initialState: AutomationState = {
  automations: [],
  automationHistory: [],
  selectedAutomationId: null,
  automationScript: null,
  isLoadingScript: false,
  isPlaying: false,
  playbackProgress: 0,
  isEditing: false,
  isRecording: false,
  isProcessingRecording: false,
};

// Atom
export const automationAtom = atomWithReducer<AutomationState, AutomationAction>(
  initialState,
  automationReducer
);

export const useAutomation = () => {
  const [state, dispatch] = useAtom(automationAtom);

  const fetchAutomations = async () => {
    const automations = await automationService.fetchAutomations();
    dispatch({ type: "setAutomations", payload: automations });
  };

  const fetchAutomationHistory = async () => {
    const history = await automationService.fetchAutomationHistory();
    dispatch({ type: "setAutomationHistory", payload: history });
  };

  const selectAutomation = (automationId: number | null) => {
    dispatch({ type: "selectAutomation", payload: automationId });
    if (automationId !== null) {
      fetchAutomationScript(automationId);
    }
  };

  const fetchAutomationScript = async (automationId: number) => {
    try {
      const script = await automationService.fetchAutomationScript(automationId);
      
      // Also fetch the automation details to get the nl_description and objective
      const automationDetails = await automationService.fetchAutomationById(automationId);
      
      if (automationDetails) {
        // Merge the script with automation details
        const enrichedScript = {
          ...script,
          objective: automationDetails.objective || script.objective,
          nl_description: automationDetails.nl_description
        };
        
        dispatch({ type: "setAutomationScript", payload: enrichedScript });
      } else {
        dispatch({ type: "setAutomationScript", payload: script });
      }
    } catch (error) {
      console.error("Error fetching automation script:", error);
      dispatch({ type: "setAutomationScript", payload: null });
    }
  };

  const startRecording = async (objective: string, name: string) => {
    try {
      await automationService.startRecordingAutomation(objective, name);
      dispatch({ type: "setIsRecording", payload: true });
      return true;
    } catch (error) {
      console.error("Error starting recording:", error);
      return false;
    }
  };

  const stopRecording = async () => {
    try {
      console.log("🔴 Calling stopRecordingAutomation service...");
      await automationService.stopRecordingAutomation();
      console.log("🟢 stopRecordingAutomation service completed");
      dispatch({ type: "setIsRecording", payload: false });
      console.log("✅ Dispatched setIsRecording(false)");
      return true;
    } catch (error) {
      console.error("Error stopping recording:", error);
      return false;
    }
  };

  const playAutomation = async (automationId: number, additionalInstructions?: string, agentMode?: boolean) => {
    try {
      await automationService.playAutomation(automationId, additionalInstructions, agentMode);
      dispatch({ type: "setIsPlaying", payload: true });
      return true;
    } catch (error) {
      console.error("Error playing automation:", error);
      return false;
    }
  };

  const stopPlayback = async () => {
    try {
      await automationService.stopAutomationPlayback();
      dispatch({ type: "setIsPlaying", payload: false });
      return true;
    } catch (error) {
      console.error("Error stopping playback:", error);
      return false;
    }
  };

  const saveAutomationScript = async (script: AutomationScript) => {
    try {
      if (state.selectedAutomationId) {
        await automationService.updateAutomationScript(state.selectedAutomationId, script);
        dispatch({ type: "setAutomationScript", payload: script });
        dispatch({ type: "setIsEditing", payload: false });
        
        // Update automations list if name was changed
        if (state.automations.find(a => a.id === state.selectedAutomationId)?.name !== script.name) {
          await fetchAutomations();
          await fetchAutomationHistory();
        }
        return true;
      }
      return false;
    } catch (error) {
      console.error("Error saving automation script:", error);
      return false;
    }
  };

  const deleteAutomation = async (automationId: number) => {
    try {
      await automationService.deleteAutomation(automationId);
      dispatch({ type: "deleteAutomation", payload: automationId });
      return true;
    } catch (error) {
      console.error("Error deleting automation:", error);
      return false;
    }
  };

  const createNewAutomation = () => {
    dispatch({ type: "selectAutomation", payload: null });
  };

  const setIsEditing = (isEditing: boolean) => {
    dispatch({ type: "setIsEditing", payload: isEditing });
  };

  // Set up event listeners for recording and playback status
  useEffect(() => {
    fetchAutomations();
    fetchAutomationHistory();
    
    // Listen for recording status changes (combined listener for all recording events)
    const unlisten1 = listen("recording_status", (event: any) => {
      console.log("📡 Recording status event received:", event.payload);
      
      // Handle isRecording state change
      if (event.payload.isRecording !== undefined) {
        dispatch({ type: "setIsRecording", payload: event.payload.isRecording });
      }
      
      // Handle isProcessing state change
      if (event.payload.isProcessing !== undefined) {
        dispatch({ type: "setIsProcessingRecording", payload: event.payload.isProcessing });
      }
      
      // If recording stopped and we have an automation ID, fetch the new automation
      if (event.payload.isRecording === false && event.payload.automationId) {
        console.log("📦 Recording stopped, fetching automation:", event.payload.automationId);
        console.log("📦 Processing state:", event.payload.isProcessing);
        fetchAutomations();
        fetchAutomationHistory();
        
        // Only select the automation if we're not still processing
        if (!event.payload.isProcessing) {
          selectAutomation(event.payload.automationId);
        }
      }
    });
    
    // Listen for playback status changes
    const unlisten2 = listen("playback_status", (event: any) => {
      dispatch({ type: "setIsPlaying", payload: event.payload.isPlaying });
      dispatch({ type: "setPlaybackProgress", payload: event.payload.progress || 0 });
      
      if (!event.payload.isPlaying) {
        // Playback finished or stopped
        fetchAutomationHistory();
      }
    });
    
    // Listen for script generation errors
    const unlisten3 = listen("script_generation_error", () => {
      dispatch({ type: "setIsProcessingRecording", payload: false });
    });

    // Listen for automation stopped — safety net to clear isPlaying
    // (in case playback_status doesn't fire or arrives with wrong value)
    const unlisten4 = listen("automation_stopped", () => {
      dispatch({ type: "setIsPlaying", payload: false });
      dispatch({ type: "setPlaybackProgress", payload: 0 });
      fetchAutomationHistory();
    });

    // Listen for automation failed — safety net to clear isPlaying on errors
    const unlisten5 = listen("automation_failed", () => {
      dispatch({ type: "setIsPlaying", payload: false });
      dispatch({ type: "setPlaybackProgress", payload: 0 });
      fetchAutomationHistory();
    });

    return () => {
      unlisten1.then(fn => fn());
      unlisten2.then(fn => fn());
      unlisten3.then(fn => fn());
      unlisten4.then(fn => fn());
      unlisten5.then(fn => fn());
    };
  }, []);

  return {
    state: {
      ...state,
      isProcessingRecording: state.isProcessingRecording,
      isLoadingScript: state.isLoadingScript
    },
    fetchAutomations,
    fetchAutomationHistory,
    selectAutomation,
    fetchAutomationScript,
    startRecording,
    stopRecording,
    playAutomation,
    stopPlayback,
    saveAutomationScript,
    deleteAutomation,
    createNewAutomation,
    setIsEditing
  };
}; 