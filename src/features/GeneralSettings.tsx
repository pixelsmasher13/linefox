import { useCallback, useEffect, useState } from "react";
import {
  Box,
  Flex,
  Text,
  Switch,
  Select,
  VStack,
  Input,
  Button,
  HStack,
  Badge,
  useToast,
} from "@chakra-ui/react";
import { invoke } from "@tauri-apps/api/core";
import { useGlobalSettings } from "../Providers/SettingsProvider";

type CodexReasoningEffort = "minimal" | "low" | "medium" | "high" | "xhigh";

type LocalSettings = {
  autoStart: boolean;
  apiChoice:
    | "claude"
    | "claude-subscription"
    | "openai"
    | "openai-codex"
    | "grok"
    | "gemini"
    | "deepseek";
  apiKeyOpenAi: string;
  apiKeyClaude: string;
  /// `sk-ant-oat01-...` OAuth token obtained via `claude setup-token`.
  /// Stored separately from `apiKeyClaude` so the user can keep both an API
  /// key and a subscription token configured and switch between them via the
  /// API Choice dropdown.
  apiKeyClaudeOauth: string;
  apiKeyGrok: string;
  apiKeyGemini: string;
  apiKeyDeepseek: string;
  storeTaskData: boolean;
  modelClaude: string;
  modelOpenai: string;
  modelOpenaiCodex: string;
  openaiCodexReasoningEffort: CodexReasoningEffort;
  modelGrok: string;
  modelGemini: string;
  modelDeepseek: string;
};

const DEFAULT_MODELS: Record<string, string> = {
  claude: "claude-sonnet-4-5-20250929",
  // Subscription path hits the same /v1/messages endpoint with the same model
  // identifiers, just authenticated via Bearer OAuth instead of x-api-key.
  "claude-subscription": "claude-sonnet-4-5-20250929",
  openai: "gpt-5",
  "openai-codex": "gpt-5.5",
  grok: "grok-3",
  gemini: "gemini-2.5-flash",
  deepseek: "deepseek-chat",
};

type CodexStatus = {
  logged_in: boolean;
  account_id?: string | null;
  expires_ms?: number | null;
};

const normalizeEffort = (v: string): CodexReasoningEffort => {
  switch (v) {
    case "minimal":
    case "low":
    case "medium":
    case "high":
    case "xhigh":
      return v;
    default:
      return "medium";
  }
};

export const GeneralSettings = () => {
  const toast = useToast();
  const { settings, update } = useGlobalSettings();
  const [localSettings, setLocalSettings] = useState<LocalSettings>({
    autoStart: settings.auto_start,
    apiChoice: settings.api_choice,
    apiKeyOpenAi: settings.api_key_open_ai,
    apiKeyClaude: settings.api_key_claude,
    apiKeyClaudeOauth: settings.api_key_claude_oauth,
    apiKeyGrok: settings.api_key_grok,
    apiKeyGemini: settings.api_key_gemini,
    apiKeyDeepseek: settings.api_key_deepseek,
    storeTaskData: settings.store_task_data,
    modelClaude: settings.model_claude,
    modelOpenai: settings.model_openai,
    modelOpenaiCodex: settings.model_openai_codex,
    openaiCodexReasoningEffort: normalizeEffort(
      settings.openai_codex_reasoning_effort
    ),
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
      apiKeyClaudeOauth: settings.api_key_claude_oauth,
      apiKeyGrok: settings.api_key_grok,
      apiKeyGemini: settings.api_key_gemini,
      apiKeyDeepseek: settings.api_key_deepseek,
      storeTaskData: settings.store_task_data,
      modelClaude: settings.model_claude,
      modelOpenai: settings.model_openai,
      modelOpenaiCodex: settings.model_openai_codex,
      openaiCodexReasoningEffort: normalizeEffort(
        settings.openai_codex_reasoning_effort
      ),
      modelGrok: settings.model_grok,
      modelGemini: settings.model_gemini,
      modelDeepseek: settings.model_deepseek,
    });
  }, [settings]);

  // ChatGPT subscription (Codex OAuth) status — fetched via Tauri command,
  // not part of the Settings struct because credentials are managed outside
  // the regular settings table by the OAuth flow.
  const [codexStatus, setCodexStatus] = useState<CodexStatus>({
    logged_in: false,
  });
  const [codexBusy, setCodexBusy] = useState(false);

  const refreshCodexStatus = useCallback(async () => {
    try {
      const status = await invoke<CodexStatus>("openai_codex_status");
      setCodexStatus(status);
    } catch (e) {
      console.warn("Failed to load ChatGPT subscription status:", e);
    }
  }, []);

  useEffect(() => {
    void refreshCodexStatus();
  }, [refreshCodexStatus]);

  const handleCodexLogin = async () => {
    setCodexBusy(true);
    try {
      await invoke("openai_codex_login");
      await refreshCodexStatus();
      toast({
        title: "Signed in to ChatGPT",
        status: "success",
        duration: 2500,
        isClosable: true,
      });
    } catch (e: any) {
      toast({
        title: "ChatGPT sign-in failed",
        description: typeof e === "string" ? e : e?.message ?? "Unknown error",
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setCodexBusy(false);
    }
  };

  const handleCodexLogout = async () => {
    setCodexBusy(true);
    try {
      await invoke("openai_codex_logout");
      await refreshCodexStatus();
      toast({
        title: "Signed out of ChatGPT",
        status: "info",
        duration: 2000,
        isClosable: true,
      });
    } catch (e: any) {
      toast({
        title: "ChatGPT sign-out failed",
        description: typeof e === "string" ? e : e?.message ?? "Unknown error",
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setCodexBusy(false);
    }
  };

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

  type ApiChoice =
    | "claude"
    | "claude-subscription"
    | "openai"
    | "openai-codex"
    | "grok"
    | "gemini"
    | "deepseek";
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
  const onChangeClaudeOauthToken = (
    event: React.ChangeEvent<HTMLInputElement>
  ) => {
    // Strip ALL whitespace, not just edges: `claude setup-token` prints the
    // token wrapped across two terminal lines, so a clipboard paste often
    // contains an embedded `\n` between the two halves. Plain `.trim()` only
    // handles leading/trailing whitespace and would silently ship a broken
    // token to Anthropic (-> 401 Invalid bearer token).
    setLocalSettings((prevState) => ({
      ...prevState,
      apiKeyClaudeOauth: event.target.value.replace(/\s+/g, ""),
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
  const onChangeDeepseekApiKey = (
    event: React.ChangeEvent<HTMLInputElement>
  ) => {
    setLocalSettings((prevState) => ({
      ...prevState,
      apiKeyDeepseek: event.target.value,
    }));
  };

  const modelFieldKey = {
    claude: "modelClaude",
    // Subscription path uses the same model identifier space as the API-key
    // path; both hit the same Anthropic /v1/messages endpoint.
    "claude-subscription": "modelClaude",
    openai: "modelOpenai",
    "openai-codex": "modelOpenaiCodex",
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
      api_key_claude_oauth: localSettings.apiKeyClaudeOauth,
      api_key_grok: localSettings.apiKeyGrok,
      api_key_gemini: localSettings.apiKeyGemini,
      api_key_deepseek: localSettings.apiKeyDeepseek,
      use_pro_model: settings.use_pro_model,
      store_task_data: localSettings.storeTaskData,
      model_claude: localSettings.modelClaude,
      model_openai: localSettings.modelOpenai,
      model_openai_codex: localSettings.modelOpenaiCodex,
      openai_codex_reasoning_effort: localSettings.openaiCodexReasoningEffort,
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
                <option value="claude">Claude (API key)</option>
                <option value="claude-subscription">
                  Claude (Subscription)
                </option>
                <option value="openai">OpenAI (API key)</option>
                <option value="openai-codex">ChatGPT (Subscription)</option>
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

          {localSettings.apiChoice === "openai-codex" && (
            <>
              <Flex alignItems="center" mb={2}>
                <Flex flex={1}>
                  <Text fontSize="md" mr={4}>
                    Reasoning effort:
                  </Text>
                </Flex>
                <Flex flex={2}>
                  <Select
                    size="md"
                    value={localSettings.openaiCodexReasoningEffort}
                    onChange={(e) =>
                      setLocalSettings((prev) => ({
                        ...prev,
                        openaiCodexReasoningEffort: e.target
                          .value as CodexReasoningEffort,
                      }))
                    }
                  >
                    <option value="minimal">Minimal (fastest)</option>
                    <option value="low">Low</option>
                    <option value="medium">Medium (default)</option>
                    <option value="high">High</option>
                    <option value="xhigh">Extra high (slowest)</option>
                  </Select>
                </Flex>
              </Flex>
              <Text fontSize="xs" color="gray.500" mb={2}>
                Higher effort = better decisions on complex steps but slower
                responses and more billed minutes against your ChatGPT
                subscription. <code>gpt-5.5</code> clamps "minimal" to "low".
              </Text>
            </>
          )}

          {localSettings.apiChoice !== "openai-codex" &&
            localSettings.apiChoice !== "claude-subscription" && (
              <Flex alignItems="center" mb={2}>
                <Flex flex={1}>
                  <Text fontSize="md" mr={4}>
                    API Key:
                  </Text>
                </Flex>
                <Flex flex={2}>
                  {localSettings.apiChoice === "openai" && (
                    <Input
                      value={localSettings.apiKeyOpenAi}
                      onChange={onChangeOpenAiApiKey}
                      type="password"
                    />
                  )}
                  {localSettings.apiChoice === "claude" && (
                    <Input
                      value={localSettings.apiKeyClaude}
                      onChange={onChangeClaueApiKey}
                      type="password"
                    />
                  )}
                  {localSettings.apiChoice === "grok" && (
                    <Input
                      value={localSettings.apiKeyGrok}
                      onChange={onChangeGrokApiKey}
                      type="password"
                    />
                  )}
                  {localSettings.apiChoice === "gemini" && (
                    <Input
                      value={localSettings.apiKeyGemini}
                      onChange={onChangeGeminiApiKey}
                      type="password"
                    />
                  )}
                  {localSettings.apiChoice === "deepseek" && (
                    <Input
                      value={localSettings.apiKeyDeepseek}
                      onChange={onChangeDeepseekApiKey}
                      type="password"
                    />
                  )}
                </Flex>
              </Flex>
            )}

          {localSettings.apiChoice === "claude-subscription" && (
            <Box mt={2}>
              <Flex alignItems="center" mb={2}>
                <Flex flex={1}>
                  <Text fontSize="md" mr={4}>
                    OAuth Token:
                  </Text>
                </Flex>
                <Flex flex={2} direction="column" gap={2}>
                  <Input
                    value={localSettings.apiKeyClaudeOauth}
                    onChange={onChangeClaudeOauthToken}
                    type="password"
                    placeholder="sk-ant-oat01-..."
                    fontFamily="mono"
                    fontSize="sm"
                  />
                  <HStack spacing={2}>
                    {localSettings.apiKeyClaudeOauth.startsWith(
                      "sk-ant-oat01-"
                    ) ? (
                      <Badge colorScheme="green">Token looks valid</Badge>
                    ) : localSettings.apiKeyClaudeOauth.length > 0 ? (
                      <Badge colorScheme="orange">
                        Expected prefix sk-ant-oat01-
                      </Badge>
                    ) : (
                      <Badge colorScheme="gray">Not configured</Badge>
                    )}
                    {localSettings.apiKeyClaudeOauth.length > 0 && (
                      <Badge colorScheme="gray" variant="outline">
                        {localSettings.apiKeyClaudeOauth.length} chars
                      </Badge>
                    )}
                  </HStack>
                </Flex>
              </Flex>
              <Box
                mt={2}
                p={3}
                bg="gray.50"
                borderRadius="md"
                borderWidth="1px"
                borderColor="gray.200"
              >
                <Text fontSize="sm" mb={2}>
                  How to get your token:
                </Text>
                <Text fontSize="xs" color="gray.600" mb={2}>
                  1. Install the Claude CLI if you haven't already:{" "}
                  <code>npm install -g @anthropic-ai/claude-code</code>
                </Text>
                <Text fontSize="xs" color="gray.600" mb={2}>
                  2. Run this command in your terminal — it opens a browser,
                  signs you in to your Claude Pro/Max account, and prints a
                  token:
                </Text>
                <HStack spacing={2} mb={2}>
                  <Box
                    as="code"
                    flex={1}
                    p={2}
                    bg="gray.900"
                    color="green.200"
                    borderRadius="sm"
                    fontSize="xs"
                    fontFamily="mono"
                  >
                    claude setup-token
                  </Box>
                  <Button
                    size="xs"
                    onClick={async () => {
                      try {
                        await navigator.clipboard.writeText(
                          "claude setup-token"
                        );
                        toast({
                          title: "Command copied",
                          status: "success",
                          duration: 1500,
                          isClosable: true,
                        });
                      } catch {
                        toast({
                          title: "Copy failed",
                          description:
                            "Couldn't write to clipboard. Copy manually.",
                          status: "warning",
                          duration: 2500,
                          isClosable: true,
                        });
                      }
                    }}
                  >
                    Copy
                  </Button>
                </HStack>
                <Text fontSize="xs" color="gray.600">
                  3. Paste the resulting <code>sk-ant-oat01-...</code> token
                  above. Calls will bill against your Claude subscription
                  instead of API credits.
                </Text>
              </Box>
            </Box>
          )}

          {localSettings.apiChoice === "openai-codex" && (
            <Flex alignItems="center" mb={2}>
              <Flex flex={1}>
                <Text fontSize="md" mr={4}>
                  ChatGPT:
                </Text>
              </Flex>
              <Flex flex={2}>
                <HStack spacing={3} width="100%">
                  {codexStatus.logged_in ? (
                    <>
                      <Badge colorScheme="green">Signed in</Badge>
                      {codexStatus.account_id && (
                        <Text
                          fontSize="xs"
                          color="gray.500"
                          fontFamily="mono"
                          isTruncated
                          maxW="180px"
                          title={codexStatus.account_id}
                        >
                          {codexStatus.account_id}
                        </Text>
                      )}
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={handleCodexLogout}
                        isLoading={codexBusy}
                      >
                        Sign out
                      </Button>
                    </>
                  ) : (
                    <Button
                      size="sm"
                      colorScheme="blue"
                      onClick={handleCodexLogin}
                      isLoading={codexBusy}
                    >
                      Sign in with ChatGPT
                    </Button>
                  )}
                </HStack>
              </Flex>
            </Flex>
          )}

          <Text fontSize="sm" color="gray.500">
            {localSettings.apiChoice === "openai-codex"
              ? `Uses your ChatGPT Plus/Pro subscription. Leave Model blank to use the default (${DEFAULT_MODELS[localSettings.apiChoice]}).`
              : localSettings.apiChoice === "claude-subscription"
                ? `Uses your Claude Pro/Max subscription via the OAuth token from \`claude setup-token\`. Leave Model blank to use the default (${DEFAULT_MODELS[localSettings.apiChoice]}).`
                : `Select the AI provider and optionally override the model. Leave Model blank to use the default (${DEFAULT_MODELS[localSettings.apiChoice]}).`}
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
