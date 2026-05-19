import React from "react";
import { Flex, Box, VStack, HStack, Spinner, Text as ChakraText } from "@chakra-ui/react";
import { CheckCircle, AlertCircle } from "lucide-react";
import { ChatMessage, ExecutionStepMetadata } from "../../chatTypes";
import { TerminalOutput } from "../../../../components/TerminalOutput";

interface ExecutionStepBubbleProps {
  message: ChatMessage;
}

export const ExecutionStepBubble: React.FC<ExecutionStepBubbleProps> = ({ message }) => {
  const meta = message.metadata as ExecutionStepMetadata;
  if (!meta) return null;

  const getStatusIcon = () => {
    switch (meta.status) {
      case "executing":
        return <Spinner size="xs" color="blue.500" />;
      case "completed":
        return <CheckCircle size={14} color="#22c55e" />;
      case "error":
        return <AlertCircle size={14} color="#ef4444" />;
      default:
        return <Spinner size="xs" color="blue.500" />;
    }
  };

  return (
    <Flex justify="flex-start" mb={2}>
      <Box
        bg="white"
        border="1px solid"
        borderColor={meta.status === "error" ? "red.200" : meta.status === "executing" ? "blue.200" : "gray.200"}
        px={4}
        py={3}
        borderRadius="xl"
        borderTopLeftRadius="sm"
        maxW="90%"
        transition="border-color 0.2s"
      >
        <VStack align="stretch" spacing={2}>
          <HStack spacing={2} align="start">
            <Box mt="2px" flexShrink={0}>
              {getStatusIcon()}
            </Box>
            <ChakraText fontSize="sm" color="gray.700">
              {meta.explanation}
            </ChakraText>
          </HStack>

          {meta.nextStep && meta.status === "executing" && (
            <Box
              bg="blue.50"
              borderLeft="3px solid"
              borderColor="blue.400"
              px={3}
              py={2}
              borderRadius="md"
            >
              <ChakraText fontSize="xs" color="blue.700">
                Next: {meta.nextStep}
              </ChakraText>
            </Box>
          )}

          {meta.error && (
            <Box
              bg="red.50"
              borderLeft="3px solid"
              borderColor="red.400"
              px={3}
              py={2}
              borderRadius="md"
            >
              <ChakraText fontSize="xs" color="red.700">
                {meta.error}
              </ChakraText>
            </Box>
          )}

          {meta.terminalProcessId && (
            <Box mt={1}>
              <TerminalOutput
                processId={meta.terminalProcessId}
                command={meta.terminalCommand}
              />
            </Box>
          )}
        </VStack>
      </Box>
    </Flex>
  );
};
