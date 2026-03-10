// OnboardingScreen.tsx
import React, { useState, useEffect } from "react";
import styled from "styled-components";
import { motion, AnimatePresence } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { type } from "@tauri-apps/plugin-os";
import { Input, Select } from "@chakra-ui/react";
import { useGlobalSettings } from "@/Providers/SettingsProvider";

const KeyContainer = styled.div`
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 10px;
`;

const OnboardingContainer = styled(motion.div)`
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100vh;
  text-align: center;
  background: linear-gradient(135deg, #1c4e80, #2980b9, #6dd5fa);
  color: white;
`;

const ContentWrapper = styled(motion.div)`
  max-width: 800px;
  padding: 2rem;
`;

const Title = styled(motion.h1)`
  font-size: 2.5rem;
  margin-bottom: 1rem;
`;

const Content = styled(motion.p)`
  font-size: 1.2rem;
  margin-bottom: 2rem;
`;

const Button = styled(motion.button)`
  padding: 12px 24px;
  font-size: 1rem;
  background-color: white;
  color: #6e8efb;
  border: none;
  border-radius: 30px;
  cursor: pointer;
  transition: all 0.3s ease;

  &:hover {
    transform: translateY(-2px);
    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.1);
  }
`;

const ProgressDots = styled.div`
  display: flex;
  justify-content: center;
  margin-top: 2rem;
`;

const Dot = styled(motion.div)`
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background-color: white;
  margin: 0 5px;
`;

const initialSteps = [
  {
    title: "Welcome to Linefox",
    content: "The AI agent that makes repetitive work disappear. One and done.",

  },
  {
    title: "Tell Linefox what to do",
    content: "Just describe what you need. Linefox works across any apps and sites.",
  },
  {
    title: "Just tell it what to do.",

    content:
      "Describe the task in plain language and Linefox figures out the rest. Set it on repeat and it keeps going — no recording, no setup, no babysitting.",
  },
  {
    title: "See what's possible",
    content: "Here's what people are doing with Linefox:",

    prompts: [
      "Collect new LinkedIn profiles and draft personalized emails to each prospect",
      "Find 100 recent 1BR condo sales in Austin and create an Excel with comparison details",
      "Review published marketing dashboards and update client spreadsheets",
      "Go through customer emails for the day and add them to the CRM system",
    ],
  },
  {
    title: "Run the way you want to",
    content: "Connect directly to Claude, OpenAI, Grok, or Gemini with your own API key. Everything stays on your machine — complete privacy, total control. You can even send tasks straight from Telegram or Discord.",
  },
];

interface OnboardingScreenProps {
  onComplete: () => void;
}

export const OnboardingScreen: React.FC<OnboardingScreenProps> = ({
  onComplete,
}) => {
  const { settings, update } = useGlobalSettings();
  const [steps, setSteps] = useState(initialSteps);
  const [step, setStep] = useState(0);
  const [isMacOS, setIsMacOS] = useState(false);

  // useEffect(() => {
  //   const checkPlatform = async () => {
  //     try {
  //       const osType = await type();
  //       if (osType === "Darwin") {
  //         setIsMacOS(true);
  //         setSteps([
  //           ...initialSteps.slice(0, -1),
  //           {
  //             title: "Permissions",
  //             content:
  //               "Heelix needs accessibility permissions to function properly. This allows us to collect text data from approved apps to provide you with personalized assistance.",
  //           },
  //           initialSteps[initialSteps.length - 1],
  //         ]);
  //       }
  //     } catch (error) {
  //       console.error("Error checking OS type:", error);
  //     }
  //   };

  //   checkPlatform();
  // }, []);

  type ApiChoice = "claude" | "openai";
  const handleApiChoiceChange = (value: ApiChoice) => {
    update({ ...settings, api_choice: value });
  };

  const onChangeApiKey = (value: string) => {
    if (settings.api_choice === "claude") {
      update({ ...settings, api_key_claude: value });
    }
    if (settings.api_choice === "openai") {
      update({ ...settings, api_key_open_ai: value });
    }
  };

  const nextStep = async () => {
    if (
      step &&
      steps.length - 1 &&
      !settings.api_key_claude &&
      !settings.api_key_open_ai
    ) {
      console.error("API key not set");
    }

//  if (isMacOS && step === steps.length - 2) {
 //     try {
 //       await invoke("prompt_for_accessibility_permissions");
 //     } catch (error) {
 //       console.error("Error requesting permissions:", error);
 //       // You might want to handle this error, perhaps by showing a message to the user
 //     }
 //   }

    if (step < steps.length - 1) {
      setStep(step + 1);
    } else {
      onComplete();
    }
  };

  return (
    <OnboardingContainer
      initial={{ opacity: 1 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
    >
      <ContentWrapper>
        <AnimatePresence mode="wait">
          <motion.div
            key={step}
            initial={{ opacity: 0, y: 50 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -50 }}
            transition={{ duration: 0.5 }}
          >
            <Title>{steps[step].title}</Title>
            <Content>{steps[step].content}</Content>
            {step < steps.length - 1 ? (
              steps[step].prompts && (
                <PromptsContainer>
                  {steps[step]?.prompts?.map((prompt, index) => (
                    <Prompt key={index}>{prompt}</Prompt>
                  ))}
                </PromptsContainer>
              )
            ) : (
              <KeyContainer>
                <Select
                  size="md"
                  value={settings.api_choice}
                  onChange={(event) =>
                    handleApiChoiceChange(event.target.value as ApiChoice)
                  }
                >
                  <option value="claude">Claude</option>
                  <option value="openai">OpenAI</option>
                </Select>

                <Input
                  placeholder="Api key"
                  style={{ color: "white" }}
                  value={
                    settings.api_choice === "claude"
                      ? settings.api_key_claude
                      : settings.api_key_open_ai
                  }
                  onChange={(event) => onChangeApiKey(event.target.value)}
                  _placeholder={{ opacity: 0.8, color: "inherit" }}
                />
              </KeyContainer>
            )}
          </motion.div>
        </AnimatePresence>
        <Button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={nextStep}
        >
          {step < steps.length - 1 ? "Continue" : "Get Started"}
        </Button>
      </ContentWrapper>
      <ProgressDots>
        {steps.map((_, index) => (
          <Dot
            key={index}
            animate={{
              scale: index === step ? 1.2 : 1,
              opacity: index === step ? 1 : 0.5,
            }}
          />
        ))}
      </ProgressDots>
    </OnboardingContainer>
  );
};

// Add these new styled components
const PromptsContainer = styled.div`
  display: flex;
  flex-direction: row;
  flex-wrap: wrap;
  justify-content: center;
  gap: 10px;
  margin-top: 1rem;
  margin-bottom: 2rem;
`;

const Prompt = styled.div`
  background-color: rgba(255, 255, 255, 0.1);
  border-radius: 20px;
  padding: 8px 12px;
  font-size: 0.9rem;
  cursor: pointer;
  transition: all 0.3s ease;
  flex: 0 1 auto;
  white-space: nowrap;
  text-align: center;

  &:hover {
    background-color: rgba(255, 255, 255, 0.2);
    transform: translateY(-2px);
  }
`;
