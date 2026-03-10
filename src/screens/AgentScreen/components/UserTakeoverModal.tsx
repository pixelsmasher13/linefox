import React, { FC, useState, useEffect } from "react";
import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
  Button,
  Text,
  Box,
  Alert,
  AlertIcon,
  AlertTitle,
  AlertDescription,
  HStack,
  VStack,
  Progress,
  Code,
} from "@chakra-ui/react";
import { User, CheckCircle } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

interface UserTakeoverModalProps {
  isOpen: boolean;
  instructions: string;
  reasoning: string;
  timestamp: string;
  onComplete?: () => void;
}

export const UserTakeoverModal: FC<UserTakeoverModalProps> = ({
  isOpen,
  instructions,
  reasoning,
  timestamp,
  onComplete,
}) => {
  const [timeElapsed, setTimeElapsed] = useState(0);
  const [isCompleting, setIsCompleting] = useState(false);

  useEffect(() => {
    if (!isOpen) {
      setTimeElapsed(0);
      return;
    }

    const interval = setInterval(() => {
      setTimeElapsed((t) => t + 1);
    }, 1000);

    return () => clearInterval(interval);
  }, [isOpen]);

  const handleComplete = async () => {
    setIsCompleting(true);
    try {
      // Notify backend that takeover is complete
      await invoke("complete_takeover");
      
      // Call optional callback
      onComplete?.();
    } catch (error) {
      console.error("Error completing takeover:", error);
    } finally {
      setIsCompleting(false);
    }
  };

  const formatTime = (seconds: number): string => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={() => {}}
      closeOnOverlayClick={false}
      closeOnEsc={false}
      size="xl"
    >
      <ModalOverlay bg="rgba(0, 0, 0, 0.7)" />
      <ModalContent>
        <ModalHeader>
          <HStack spacing={3}>
            <User size={24} />
            <VStack align="start" spacing={0}>
              <Text fontSize="lg">Manual Action Required</Text>
              <Text fontSize="sm" color="gray.500">
                The agent needs your help
              </Text>
            </VStack>
          </HStack>
        </ModalHeader>

        <ModalBody>
          <Alert status="info" mb={4} borderRadius="md">
            <AlertIcon />
            <Box>
              <AlertTitle>Instructions</AlertTitle>
              <AlertDescription fontSize="md" mt={2}>
                {instructions}
              </AlertDescription>
            </Box>
          </Alert>

          {reasoning && (
            <Box mb={4} p={3} bg="gray.50" borderRadius="md">
              <Text fontSize="sm" color="gray.600" mb={1}>
                Why this is needed:
              </Text>
              <Text fontSize="sm">{reasoning}</Text>
            </Box>
          )}

          <VStack spacing={3} align="stretch">
            <Box>
              <Text fontSize="sm" color="gray.600" mb={1}>
                Time elapsed:
              </Text>
              <Code fontSize="lg" colorScheme="blue">
                {formatTime(timeElapsed)}
              </Code>
            </Box>

            <Box>
              <Text fontSize="sm" color="gray.600" mb={2}>
                Progress:
              </Text>
              <Progress
                value={(timeElapsed / 300) * 100}
                size="sm"
                colorScheme={timeElapsed > 240 ? "red" : "blue"}
                borderRadius="full"
              />
              <Text fontSize="xs" color="gray.500" mt={1}>
                Timeout in {formatTime(300 - timeElapsed)}
              </Text>
            </Box>
          </VStack>
        </ModalBody>

        <ModalFooter>
          <Button
            colorScheme="green"
            size="lg"
            leftIcon={<CheckCircle size={20} />}
            onClick={handleComplete}
            isLoading={isCompleting}
            loadingText="Continuing..."
            width="full"
          >
            I've completed the action - resume the agent
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}; 