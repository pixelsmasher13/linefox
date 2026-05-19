import React, { useState, useEffect } from "react";
import {
  Flex,
  Box,
  VStack,
  HStack,
  Textarea,
  IconButton,
  Progress,
  Text as ChakraText,
} from "@chakra-ui/react";
import { MessageCircle, Send } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { ChatMessage, ClarificationMetadata } from "../../chatTypes";

interface ClarificationInlineProps {
  message: ChatMessage;
  isLatest: boolean;
  onSubmit: (response: string) => void;
}

export const ClarificationInline: React.FC<ClarificationInlineProps> = ({
  message,
  isLatest,
  onSubmit,
}) => {
  const meta = message.metadata as ClarificationMetadata;
  const [response, setResponse] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [timeElapsed, setTimeElapsed] = useState(0);
  const showInput = isLatest && !meta?.responded;

  useEffect(() => {
    if (!showInput) return;
    const interval = setInterval(() => setTimeElapsed((t) => t + 1), 1000);
    return () => clearInterval(interval);
  }, [showInput]);

  const isPreTask = !!(message.metadata as any)?.preTaskClarification;

  const handleSubmit = async () => {
    if (!response.trim() || isSubmitting) return;
    setIsSubmitting(true);
    try {
      // Pre-task clarification: don't invoke backend (no running execution)
      if (!isPreTask) {
        await invoke("submit_clarification", { response: response.trim() });
      }
      onSubmit(response.trim());
      setResponse("");
    } catch (error) {
      console.error("Error submitting clarification:", error);
    } finally {
      setIsSubmitting(false);
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
        bg="white"
        border="1px solid"
        borderColor="gray.200"
        borderLeft="3px solid"
        borderLeftColor="blue.400"
        px={4}
        py={3}
        borderRadius="lg"
        maxW="90%"
      >
        <VStack align="stretch" spacing={3}>
          <HStack spacing={2}>
            <MessageCircle size={14} color="#3182ce" />
            <ChakraText fontSize="xs" fontWeight="semibold" color="blue.600">
              Clarifying question
            </ChakraText>
          </HStack>

          <ChakraText fontSize="sm" color="gray.800" lineHeight="1.5">
            {meta?.question}
          </ChakraText>

          {meta?.reasoning && (
            <ChakraText fontSize="xs" color="gray.500" fontStyle="italic">
              {meta.reasoning}
            </ChakraText>
          )}

          {showInput && (
            <VStack spacing={2} align="stretch">
              <HStack spacing={2}>
                <Textarea
                  value={response}
                  onChange={(e) => setResponse(e.target.value)}
                  placeholder="Type your answer..."
                  rows={2}
                  resize="none"
                  fontSize="sm"
                  bg="gray.50"
                  border="1px solid"
                  borderColor="gray.200"
                  borderRadius="lg"
                  _focus={{ borderColor: "blue.300", boxShadow: "0 0 0 1px var(--chakra-colors-blue-300)", bg: "white" }}
                  autoFocus
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      handleSubmit();
                    }
                  }}
                />
                <IconButton
                  aria-label="Send response"
                  icon={<Send size={16} />}
                  colorScheme="blue"
                  size="sm"
                  borderRadius="lg"
                  onClick={handleSubmit}
                  isLoading={isSubmitting}
                  isDisabled={!response.trim()}
                />
              </HStack>
              {!isPreTask && (
                <HStack spacing={2}>
                  <Progress
                    value={(timeElapsed / 120) * 100}
                    size="xs"
                    colorScheme={timeElapsed > 90 ? "red" : "blue"}
                    borderRadius="full"
                    flex={1}
                  />
                  <ChakraText fontSize="xs" color="gray.400">
                    {formatTime(120 - timeElapsed)}
                  </ChakraText>
                </HStack>
              )}
            </VStack>
          )}
        </VStack>
      </Box>
    </Flex>
  );
};
