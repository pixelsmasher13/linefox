import React from 'react';
import { Button, Tooltip, Flex, Box } from '@chakra-ui/react';
import { keyframes } from '@emotion/react';

// Create a pulsing animation for the recording indicator
const pulse = keyframes`
  0% { opacity: 0.7; transform: scale(0.95); }
  50% { opacity: 1; transform: scale(1.05); }
  100% { opacity: 0.7; transform: scale(0.95); }
`;

interface RecordButtonProps {
  isRecording: boolean;
  onStartRecording: () => void;
  onStopRecording: () => void;
  automationName?: string;
}

export const RecordButton: React.FC<RecordButtonProps> = ({
  isRecording,
  onStartRecording,
  onStopRecording,
  automationName,
}) => {
  // Apply the pulse animation to the record indicator when recording
  const pulseAnimation = `${pulse} 2s infinite ease-in-out`;

  return (
    <Tooltip label={isRecording ? 'Click to stop recording' : 'Show Linefox how to do a task once, then let it handle it for you'}>
      <Button
        leftIcon={
          isRecording ? (
            <Box as="span" position="relative">
              <Box
                as="span"
                display="inline-block"
                w="12px"
                h="12px"
                borderRadius="50%"
                bg="red.500"
                animation={pulseAnimation}
                mr="2px"
              />
            </Box>
          ) : undefined
        }
        onClick={() => {
          console.log("🔘 RecordButton clicked! isRecording:", isRecording);
          if (isRecording) {
            console.log("🔘 Calling onStopRecording");
            onStopRecording();
          } else {
            console.log("🔘 Calling onStartRecording");
            onStartRecording();
          }
        }}
        colorScheme={isRecording ? "red" : "gray"}
        variant={isRecording ? "solid" : "outline"}
        size="md"
        borderRadius="md"
        width="100%"
        fontWeight="medium"
        _hover={{
          borderColor: isRecording ? "red.400" : "gray.400"
        }}
        transition="all 0.2s"
      >
        {isRecording ? (
          <Flex align="center" gap="2">
            Recording{automationName ? `: ${automationName}` : '...'}
          </Flex>
        ) : (
          'Record New Task'
        )}
      </Button>
    </Tooltip>
  );
}; 