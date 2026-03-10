import { useEffect, useState, useCallback } from "react";
import {
  Box,
  Flex,
  Text,
  Button,
  Wrap,
  WrapItem,
  Badge,
} from "@chakra-ui/react";
import { invoke } from "@tauri-apps/api/core";

export const CliToolsSettings = () => {
  const [detectedTools, setDetectedTools] = useState<string[]>([]);
  const [isRefreshing, setIsRefreshing] = useState(false);

  const loadDetectedTools = useCallback(async () => {
    try {
      const tools = await invoke<string[]>("get_detected_cli_tools");
      setDetectedTools(tools);
    } catch {
      // probe may not have run yet — ignore
    }
  }, []);

  useEffect(() => {
    loadDetectedTools();
  }, [loadDetectedTools]);

  const handleRefresh = async () => {
    setIsRefreshing(true);
    try {
      const tools = await invoke<string[]>("refresh_cli_probe");
      setDetectedTools(tools);
    } finally {
      setIsRefreshing(false);
    }
  };

  return (
    <Box>
      <Flex alignItems="center" mb={2} justifyContent="space-between">
        <Text fontSize="md" fontWeight="medium">
          Detected CLI Tools
        </Text>
        <Button
          size="sm"
          variant="outline"
          isLoading={isRefreshing}
          onClick={handleRefresh}
        >
          Refresh
        </Button>
      </Flex>
      <Text fontSize="sm" color="gray.500" mb={4}>
        Tools confirmed available on this machine. The agent prefers these over
        UI interaction — if <code>claude</code> or <code>openai</code> is listed,
        it will use them for code and analysis tasks instead of opening apps.
      </Text>
      {detectedTools.length > 0 ? (
        <Wrap spacing={2}>
          {detectedTools.map((tool) => (
            <WrapItem key={tool}>
              <Badge
                colorScheme={
                  tool === "claude" || tool === "openai" ? "purple" : "green"
                }
                px={2}
                py={1}
                borderRadius="md"
                fontFamily="mono"
              >
                {tool}
              </Badge>
            </WrapItem>
          ))}
        </Wrap>
      ) : (
        <Text fontSize="sm" color="gray.400" fontStyle="italic">
          No tools detected yet — app may still be loading.
        </Text>
      )}
    </Box>
  );
};
