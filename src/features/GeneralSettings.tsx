import { useEffect, useState } from "react";
import {
  Box,
  Flex,
  Text,
  Switch,
  Select,
  VStack,
  Input,
  Button,
  useToast,
} from "@chakra-ui/react";
import { useGlobalSettings } from "../Providers/SettingsProvider";

type LocalSettings = {
  autoStart: boolean;
  apiChoice: "claude" | "openai" | "grok" | "gemini" | "deepseek";
  apiKeyOpenAi: string;
  apiKeyClaude: string;
  apiKeyGrok: string;
  apiKeyGemini: string;
  apiKeyDeepseek: string;
  storeTaskData: boolean;
  modelClaude: string;
  modelOpenai: string;
  modelGrok: string;
  modelGemini: string;
  modelDeepseek: string;
};

const DEFAULT_MODELS: Record<string, string> = {
  claude: "claude-sonnet-4-5-20250929",
  openai: "gpt-5",
  grok: "grok-3",
  gemini: "gemini-2.5-flash",
  deepseek: "deepseek-chat",
};
export const GeneralSettings = () => {
  const toast = useToast();
  const { settings, update } = useGlobalSettings();
  const [localSettings, setLocalSettings] = useState<LocalSettings>({
    autoStart: settings.auto_start,
    apiChoice: settings.api_choice,
    apiKeyOpenAi: settings.api_key_open_ai,
    apiKeyClaude: settings.api_key_claude,
    apiKeyGrok: settings.api_key_grok,
    apiKeyGemini: settings.api_key_gemini,
    apiKeyDeepseek: settings.api_key_deepseek,
    storeTaskData: settings.store_task_data,
    modelClaude: settings.model_claude,
    modelOpenai: settings.model_openai,
    modelGrok: settings.model_grok,
    modelGemini: settings.model_gemini,
    modelDeepseek: settings.model_deepseek,
  });

  useEffect(() => {
    setLocalSettings({
      autoStart: settings.auto_start,
      apiChoice: settings.api_choice,
      apiKeyOpenAi: settings.api_key_open_ai,
      apiKeyClaude: settings.api_key_claude,
      apiKeyGrok: settings.api_key_grok,
      apiKeyGemini: settings.api_key_gemini,
      apiKeyDeepseek: settings.api_key_deepseek,
      storeTaskData: settings.store_task_data,
      modelClaude: settings.model_claude,
      modelOpenai: settings.model_openai,
      modelGrok: settings.model_grok,
      modelGemini: settings.model_gemini,
      modelDeepseek: settings.model_deepseek,
    });
  }, [settings]);

  const savedSuccessfullyToast = () => {
    toast({
      title: "Setttings saved sucessfully",
      status: "success",
      duration: 2000,
      isClosable: true,
    });
  };

  const handleAutoStartChange = async (
    event: React.ChangeEvent<HTMLInputElement>
  ) => {
    const isChecked = event.target.checked;
    await update({ ...settings, auto_start: isChecked });
  };

  type ApiChoice = "claude" | "openai" | "grok" | "gemini" | "deepseek";
  const handleApiChoiceChange = async (
    event: React.ChangeEvent<HTMLSelectElement>
  ) => {
    const apiChoice = event.target.value as ApiChoice;
    setLocalSettings((prevState) => ({ ...prevState, apiChoice }));
  };

  const onChangeOpenAiApiKey = (event: React.ChangeEvent<HTMLInputElement>) => {
    setLocalSettings((prevState) => ({
      ...prevState,
      apiKeyOpenAi: event.target.value,
    }));
  };
  const onChangeClaueApiKey = (event: React.ChangeEvent<HTMLInputElement>) => {
    setLocalSettings((prevState) => ({
      ...prevState,
      apiKeyClaude: event.target.value,
    }));
  };
  const onChangeGrokApiKey = (event: React.ChangeEvent<HTMLInputElement>) => {
    setLocalSettings((prevState) => ({
      ...prevState,
      apiKeyGrok: event.target.value,
    }));
  };
  const onChangeGeminiApiKey = (event: React.ChangeEvent<HTMLInputElement>) => {
    setLocalSettings((prevState) => ({
      ...prevState,
      apiKeyGemini: event.target.value,
    }));
  };
  const onChangeDeepseekApiKey = (event: React.ChangeEvent<HTMLInputElement>) => {
    setLocalSettings((prevState) => ({
      ...prevState,
      apiKeyDeepseek: event.target.value,
    }));
  };

  const modelFieldKey = {
    claude: "modelClaude",
    openai: "modelOpenai",
    grok: "modelGrok",
    gemini: "modelGemini",
    deepseek: "modelDeepseek",
  } as const;

  const currentModelValue =
    localSettings[modelFieldKey[localSettings.apiChoice]];

  const handleModelChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const field = modelFieldKey[localSettings.apiChoice];
    setLocalSettings((prev) => ({ ...prev, [field]: e.target.value }));
  };

  const onSave = () => {
    update({
      ...settings,
      auto_start: localSettings.autoStart,
      api_choice: localSettings.apiChoice,
      api_key_open_ai: localSettings.apiKeyOpenAi,
      api_key_claude: localSettings.apiKeyClaude,
      api_key_grok: localSettings.apiKeyGrok,
      api_key_gemini: localSettings.apiKeyGemini,
      api_key_deepseek: localSettings.apiKeyDeepseek,
      use_pro_model: settings.use_pro_model,
      store_task_data: localSettings.storeTaskData,
      model_claude: localSettings.modelClaude,
      model_openai: localSettings.modelOpenai,
      model_grok: localSettings.modelGrok,
      model_gemini: localSettings.modelGemini,
      model_deepseek: localSettings.modelDeepseek,
    });
    savedSuccessfullyToast();
  };
  return (
    <Box>
      <VStack spacing={8} align="stretch">
        <Box>
          <Flex alignItems="center" mb={2}>
            <Text fontSize="md" mr={4}>
              Autostart Linefox:
            </Text>
            <Switch
              size="md"
              isChecked={localSettings.autoStart}
              onChange={handleAutoStartChange}
            />
          </Flex>
          <Text fontSize="sm" color="gray.500">
            Enable this option to automatically start the application on system
            startup.
          </Text>
        </Box>

        <Box>
          <Flex alignItems="center" mb={2}>
            <Flex flex={1}>
              <Text fontSize="md" mr={4}>
                API Choice:
              </Text>
            </Flex>
            <Flex flex={2}>
              <Select
                size="md"
                value={localSettings.apiChoice}
                onChange={handleApiChoiceChange}
              >
                <option value="claude">Claude</option>
                <option value="openai">OpenAI</option>
                <option value="grok">Grok</option>
                <option value="gemini">Gemini</option>
                <option value="deepseek">DeepSeek</option>
              </Select>
            </Flex>
          </Flex>
          <Flex alignItems="center" mb={2}>
            <Flex flex={1}>
              <Text fontSize="md" mr={4}>
                Model:
              </Text>
            </Flex>
            <Flex flex={2}>
              <Input
                value={currentModelValue}
                placeholder={DEFAULT_MODELS[localSettings.apiChoice]}
                onChange={handleModelChange}
                fontFamily="mono"
                fontSize="sm"
              />
            </Flex>
          </Flex>

          <Flex alignItems="center" mb={2}>
            <Flex flex={1}>
              <Text fontSize="md" mr={4}>
                API Key:
              </Text>
            </Flex>
            <Flex flex={2}>
              {localSettings.apiChoice === "openai" && (
                <Input value={localSettings.apiKeyOpenAi} onChange={onChangeOpenAiApiKey} type="password" />
              )}
              {localSettings.apiChoice === "claude" && (
                <Input value={localSettings.apiKeyClaude} onChange={onChangeClaueApiKey} type="password" />
              )}
              {localSettings.apiChoice === "grok" && (
                <Input value={localSettings.apiKeyGrok} onChange={onChangeGrokApiKey} type="password" />
              )}
              {localSettings.apiChoice === "gemini" && (
                <Input value={localSettings.apiKeyGemini} onChange={onChangeGeminiApiKey} type="password" />
              )}
              {localSettings.apiChoice === "deepseek" && (
                <Input value={localSettings.apiKeyDeepseek} onChange={onChangeDeepseekApiKey} type="password" />
              )}
            </Flex>
          </Flex>
          <Text fontSize="sm" color="gray.500">
            Select the AI provider and optionally override the model. Leave Model blank to use the default ({DEFAULT_MODELS[localSettings.apiChoice]}).
          </Text>

          <Box mt={4}>
            <Flex alignItems="center" mb={2}>
              <Text fontSize="md" mr={4}>
                Store Task Data:
              </Text>
              <Switch
                size="md"
                isChecked={localSettings.storeTaskData}
                onChange={(e) =>
                  setLocalSettings((prev) => ({
                    ...prev,
                    storeTaskData: e.target.checked,
                  }))
                }
              />
            </Flex>
            <Text fontSize="sm" color="gray.500">
              Automatically extract and store useful data (contacts, research, etc.) 
              from completed tasks for future reference.
            </Text>
          </Box>

          <Flex flex={1} justifyContent="flex-end" mt={4}>
            <Button colorScheme="blue" size="md" onClick={onSave}>
              Save
            </Button>
          </Flex>
        </Box>
      </VStack>
    </Box>
  );
};
