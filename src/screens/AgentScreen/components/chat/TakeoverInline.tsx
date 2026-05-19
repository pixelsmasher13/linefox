import React, { useState, useEffect } from "react";
import {
  Flex,
  Box,
  VStack,
  HStack,
  Button,
  Progress,
  Text as ChakraText,
} from "@chakra-ui/react";
import { User, CheckCircle } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { ChatMessage, TakeoverMetadata } from "../../chatTypes";

interface TakeoverInlineProps {
  message: ChatMessage;
  isLatest: boolean;
  onComplete: () => void;
}

export const TakeoverInline: React.FC<TakeoverInlineProps> = ({
  message,
  isLatest,
  onComplete,
}) => {
  const meta = message.metadata as TakeoverMetadata;
  const [isCompleting, setIsCompleting] = useState(false);
  const [timeElapsed, setTimeElapsed] = useState(0);
  const showButton = isLatest && !meta?.completed;

  useEffect(() => {
    if (!showButton) return;
    const interval = setInterval(() => setTimeElapsed((t) => t + 1), 1000);
    return () => clearInterval(interval);
  }, [showButton]);

  const handleComplete = async () => {
    setIsCompleting(true);
    try {
      await invoke("complete_takeover");
      onComplete();
    } catch (error) {
      console.error("Error completing takeover:", error);
    } finally {
      setIsCompleting(false);
    }
  };

  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  return (
    <Flex justify="flex-start" mb={3}>
      <Box
        bg="blue.50"
        border="1px solid"
        borderColor="blue.200"
        px={4}
        py={3}
        borderRadius="xl"
        borderTopLeftRadius="sm"
        maxW="90%"
      >
        <VStack align="stretch" spacing={3}>
          <HStack spacing={2}>
            <User size={16} color="#3182ce" />
            <ChakraText fontSize="xs" fontWeight="bold" color="blue.700" textTransform="uppercase">
              Manual Action Required
            </ChakraText>
          </HStack>

          <ChakraText fontSize="sm" color="gray.800">
            {meta?.instructions}
          </ChakraText>

          {meta?.reasoning && (
            <ChakraText fontSize="xs" color="gray.500" fontStyle="italic">
              {meta.reasoning}
            </ChakraText>
          )}

          {showButton && (
            <VStack spacing={2} align="stretch">
              <Button
                size="sm"
                colorScheme="green"
                leftIcon={<CheckCircle size={16} />}
                onClick={handleComplete}
                isLoading={isCompleting}
                loadingText="Continuing..."
              >
                I've completed the action - resume agent
              </Button>
              <HStack spacing={2}>
                <Progress
                  value={(timeElapsed / 300) * 100}
                  size="xs"
                  colorScheme={timeElapsed > 240 ? "red" : "blue"}
                  borderRadius="full"
                  flex={1}
                />
                <ChakraText fontSize="xs" color="gray.500">
                  {formatTime(300 - timeElapsed)}
                </ChakraText>
              </HStack>
            </VStack>
          )}
        </VStack>
      </Box>
    </Flex>
  );
};
