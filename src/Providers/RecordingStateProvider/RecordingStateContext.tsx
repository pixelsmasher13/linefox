import type { PropsWithChildren, FC } from "react";
import { createContext, useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import dayjs from "dayjs";
import { useGlobalSettings } from "../SettingsProvider";
import { useUser } from "@/state/userState";

type RecordingState = {
  isRecording: boolean;
  activityTimeActive: number;
};

type ToggleRecording = (isRecording?: RecordingState["isRecording"]) => void;

type RecordingStateContextType = {
  toggleRecording: ToggleRecording;
} & RecordingState;

const defaultRecordingState = {
  isRecording: false,
  activityTimeActive: 0,
};

export const RecordingStateContext = createContext<RecordingStateContextType>({
  ...defaultRecordingState,
  toggleRecording: () => {},
});

export const RecordingStateProvider: FC<PropsWithChildren> = ({ children }) => {
  const { settings } = useGlobalSettings();
  const { user } = useUser();

  const [isRecording, setRecording] = useState<RecordingState["isRecording"]>(
    defaultRecordingState.isRecording
  );
  const [startRecordingTime, setStartRecordingTime] = useState(0);
  const [activityTimeActive, setActivityTimeActive] = useState<
    RecordingState["activityTimeActive"]
  >(defaultRecordingState.activityTimeActive);

  // ===== Update recording time =====
  useEffect(() => {
    if (startRecordingTime) {
      const interval = setInterval(() => {
        setActivityTimeActive(dayjs().unix() - startRecordingTime);
      }, 1000); // 1 sec

      return () => clearInterval(interval);
    }
  }, [startRecordingTime]);

  useEffect(() => {
    const unlisten = listen<string>("toggle_recording", (event) => {
      // Enable toggling recording
      toggleRecording();
    });

    return () => {
      unlisten.then((f) => f());
    };
  }, [user.id]);  // Re-run when user ID changes

  const toggleRecording: ToggleRecording = (newIsRecording) => {
    // Determine new recording state
    const updatedRecordingState = newIsRecording !== undefined ? newIsRecording : !isRecording;
    
    // Update state immediately for UI responsiveness
    setRecording(updatedRecordingState);

    if (!updatedRecordingState) {
      // When stopping recording
      console.log("Stopping recording session");
      setStartRecordingTime(0);
      
      // Call to backend to stop recording
      invoke("stop_recording").catch(error => {
        console.error("Error stopping recording:", error);
      });
    } else {
      // When starting recording
      console.log("Starting recording session with realtime capture");
      setStartRecordingTime(dayjs().unix());
      
      // Start recording with real-time action capture
      invoke("record_single_activity")
        .catch(error => {
          console.error("Error starting recording:", error);
          // Revert UI state if backend call fails
          setRecording(false);
          setStartRecordingTime(0);
        });
    }
  };

  return (
    <RecordingStateContext.Provider
      value={{
        isRecording,
        activityTimeActive,
        toggleRecording,
      }}
    >
      {children}
    </RecordingStateContext.Provider>
  );
};