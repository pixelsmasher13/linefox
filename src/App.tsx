import React, { useState, useEffect } from "react";
import { AutomationScreen } from "./screens/AgentScreen";
import { OnboardingScreen } from "./screens";
import { RecordingStateProvider } from './Providers/RecordingStateProvider';
import { ColorModeManager } from './components/ColorModeManager';

export const AppContent: React.FC = () => {
  const [hasCompletedOnboarding, setHasCompletedOnboarding] = useState(false);

  useEffect(() => {
    const onboardingCompleted = localStorage.getItem("onboardingCompleted");
    if (onboardingCompleted) {
      setHasCompletedOnboarding(true);
    }
  }, []);

  const completeOnboarding = () => {
    setHasCompletedOnboarding(true);
    localStorage.setItem("onboardingCompleted", "true");
  };

  if (!hasCompletedOnboarding) {
    return <OnboardingScreen onComplete={completeOnboarding} />;
  }

  return <AutomationScreen />;
};

export const App: React.FC = () => {
  return (
    <RecordingStateProvider>
      <ColorModeManager />
      <AppContent />
    </RecordingStateProvider>
  );
};
