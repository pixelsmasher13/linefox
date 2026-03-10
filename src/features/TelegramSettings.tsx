import { useEffect, useState } from "react";
import {
  Box,
  Flex,
  Text,
  Input,
  Button,
  Badge,
  Divider,
  useToast,
} from "@chakra-ui/react";
import { invoke } from "@tauri-apps/api/core";

type TelegramConfig = {
  token_configured: boolean;
  token_masked: string;
  allowed_users: string;
  bot_running: boolean;
};

export const TelegramSettings = () => {
  const toast = useToast();
  const [botToken, setBotToken] = useState("");
  const [allowedUsers, setAllowedUsers] = useState("");
  const [config, setConfig] = useState<TelegramConfig | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [isDisconnecting, setIsDisconnecting] = useState(false);

  useEffect(() => {
    invoke<TelegramConfig>("get_telegram_config")
      .then((cfg) => {
        setConfig(cfg);
        setAllowedUsers(cfg.allowed_users);
      })
      .catch(() => {});
  }, []);

  const onSave = async () => {
    setIsSaving(true);
    try {
      if (botToken.trim()) {
        await invoke("set_telegram_bot_token", { token: botToken.trim() });
      }
      await invoke("set_telegram_allowed_users", { userIds: allowedUsers.trim() });

      // Refresh status
      const updated = await invoke<TelegramConfig>("get_telegram_config");
      setConfig(updated);
      setBotToken("");

      toast({
        title: "Telegram settings saved",
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (e) {
      toast({
        title: "Failed to save Telegram settings",
        description: String(e),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Box>
      <Divider mb={6} />

      <Flex alignItems="center" mb={5}>
        <Text fontSize="md" fontWeight="semibold" mr={3}>
          Telegram Bot
        </Text>
        {config && (
          <Badge
            colorScheme={config.bot_running ? "green" : "gray"}
            variant="subtle"
            fontSize="xs"
          >
            {config.bot_running ? "connected" : config.token_configured ? "token set" : "not configured"}
          </Badge>
        )}
      </Flex>

      <Box>
        <Flex alignItems="center" mb={2}>
          <Flex flex={1}>
            <Text fontSize="md" mr={4}>
              Bot Token:
            </Text>
          </Flex>
          <Flex flex={2}>
            <Input
              type="password"
              value={botToken}
              onChange={(e) => setBotToken(e.target.value)}
              placeholder={
                config?.token_configured
                  ? `Current: ${config.token_masked} — paste new to replace`
                  : "Paste token from @BotFather"
              }
            />
          </Flex>
        </Flex>
        <Text fontSize="sm" color="gray.500" mb={4}>
          Create a bot with @BotFather on Telegram and paste the token here.
          Linefox will start polling for messages immediately after saving.
        </Text>

        <Flex alignItems="center" mb={2}>
          <Flex flex={1}>
            <Text fontSize="md" mr={4}>
              Allowed User IDs:
            </Text>
          </Flex>
          <Flex flex={2}>
            <Input
              value={allowedUsers}
              onChange={(e) => setAllowedUsers(e.target.value)}
              placeholder="e.g. 123456789, 987654321"
            />
          </Flex>
        </Flex>
        <Text fontSize="sm" color="gray.500" mb={4}>
          Comma-separated Telegram user IDs that can run agent tasks. Find
          your ID by messaging @userinfobot. Leave empty to allow anyone (not
          recommended).
        </Text>

        <Box
          bg="gray.50"
          borderRadius="md"
          p={3}
          mb={4}
          fontSize="sm"
          color="gray.600"
        >
          <Text fontWeight="medium" mb={1}>
            How it works:
          </Text>
          <Text>
            Send any message to your bot and Linefox figures out what to do:
            saved tasks run directly, anything else is treated as a new agent
            task. You get an immediate reply when it starts and a completion
            message when it's done.
          </Text>
          <Text mt={2}>
            Commands: <Text as="span" fontFamily="mono">/list</Text> · <Text as="span" fontFamily="mono">/status</Text> · <Text as="span" fontFamily="mono">/stop</Text> · <Text as="span" fontFamily="mono">/help</Text>
          </Text>
        </Box>

        <Flex justifyContent="flex-end" gap={3}>
          {config?.token_configured && (
            <Button
              colorScheme="red"
              variant="outline"
              size="md"
              onClick={async () => {
                setIsDisconnecting(true);
                try {
                  await invoke("disconnect_telegram_bot");
                  const updated = await invoke<TelegramConfig>("get_telegram_config");
                  setConfig(updated);
                  setBotToken("");
                  toast({
                    title: "Telegram bot disconnected",
                    status: "info",
                    duration: 2000,
                    isClosable: true,
                  });
                } catch (e) {
                  toast({
                    title: "Failed to disconnect",
                    description: String(e),
                    status: "error",
                    duration: 3000,
                    isClosable: true,
                  });
                } finally {
                  setIsDisconnecting(false);
                }
              }}
              isLoading={isDisconnecting}
              loadingText="Disconnecting..."
            >
              Disconnect
            </Button>
          )}
          <Button
            colorScheme="blue"
            size="md"
            onClick={onSave}
            isLoading={isSaving}
            loadingText="Saving..."
          >
            Save
          </Button>
        </Flex>
      </Box>
    </Box>
  );
};
