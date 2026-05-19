import React, { useState, useEffect } from "react";
import { Flex, Box, HStack, Spinner, Text as ChakraText } from "@chakra-ui/react";
import { ChatMessage, PlanningMetadata } from "../../chatTypes";

const TASK_MESSAGES = [
  "Reading your mind...",
  "Mapping out the journey...",
  "Finding the perfect apps...",
  "Teaching the robots...",
  "Adding some magic...",
  "Almost there...",
];

const AGENT_MESSAGES = [
  "Analyzing your objective...",
  "Preparing the agent...",
  "Almost ready...",
];

interface PlanningIndicatorProps {
  message: ChatMessage;
}

export const PlanningIndicator: React.FC<PlanningIndicatorProps> = ({ message }) => {
  const metadata = message.metadata as PlanningMetadata | undefined;
  const agentMode = metadata?.agentMode ?? false;
  const rotatingMessages = agentMode ? AGENT_MESSAGES : TASK_MESSAGES;

  const [messageIndex, setMessageIndex] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setMessageIndex((i) => (i + 1) % rotatingMessages.length);
    }, agentMode ? 1000 : 2000);
    return () => clearInterval(interval);
  }, [agentMode, rotatingMessages.length]);

  return (
    <Flex justify="flex-start" mb={3}>
      <Box
        bg="white"
        border="1px solid"
        borderColor="gray.200"
        px={4}
        py={3}
        borderRadius="xl"
        borderTopLeftRadius="sm"
        maxW="85%"
      >
        <HStack spacing={3}>
          <Spinner
            thickness="3px"
            speed="0.65s"
            emptyColor="gray.200"
            color="purple.500"
            size="sm"
          />
          <ChakraText fontSize="sm" color="gray.600">
            {rotatingMessages[messageIndex]}
          </ChakraText>
        </HStack>
      </Box>
    </Flex>
  );
};
