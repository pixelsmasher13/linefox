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
  Textarea,
  FormControl,
  FormLabel,
  FormHelperText,
} from "@chakra-ui/react";
import { MessageCircle, Send } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

interface ClarificationModalProps {
  isOpen: boolean;
  question: string;
  reasoning: string;
  timestamp: string;
  onComplete?: () => void;
}

export const ClarificationModal: FC<ClarificationModalProps> = ({
  isOpen,
  question,
  reasoning,
  timestamp,
  onComplete,
}) => {
  const [response, setResponse] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [timeElapsed, setTimeElapsed] = useState(0);

  useEffect(() => {
    if (!isOpen) {
      setTimeElapsed(0);
      setResponse("");
      return;
    }

    const interval = setInterval(() => {
      setTimeElapsed((t) => t + 1);
    }, 1000);

    return () => clearInterval(interval);
  }, [isOpen]);

  const handleSubmit = async () => {
    if (!response.trim()) return;
    
    setIsSubmitting(true);
    try {
      // Submit clarification to backend
      await invoke("submit_clarification", { response: response.trim() });
      
      // Call optional callback
      onComplete?.();
    } catch (error) {
      console.error("Error submitting clarification:", error);
    } finally {
      setIsSubmitting(false);
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
            <MessageCircle size={24} />
            <VStack align="start" spacing={0}>
              <Text fontSize="lg">Clarification Needed</Text>
              <Text fontSize="sm" color="gray.500">
                The task needs more information
              </Text>
            </VStack>
          </HStack>
        </ModalHeader>

        <ModalBody>
          <Alert status="warning" mb={4} borderRadius="md">
            <AlertIcon />
            <Box>
              <AlertTitle>Question from Agent</AlertTitle>
              <AlertDescription fontSize="md" mt={2}>
                {question}
              </AlertDescription>
            </Box>
          </Alert>

          {reasoning && (
            <Box mb={4} p={3} bg="gray.50" borderRadius="md">
              <Text fontSize="sm" color="gray.600" mb={1}>
                Why this is being asked:
              </Text>
              <Text fontSize="sm">{reasoning}</Text>
            </Box>
          )}

          <FormControl mt={4}>
            <FormLabel>Your Response</FormLabel>
            <Textarea
              value={response}
              onChange={(e) => setResponse(e.target.value)}
              placeholder="Type your answer here..."
              rows={4}
              resize="vertical"
              autoFocus
            />
            <FormHelperText>
              Provide clear, specific information to help the agent continue
            </FormHelperText>
          </FormControl>

          <HStack mt={4} spacing={4} align="center">
            <Box>
              <Text fontSize="sm" color="gray.600">
                Time elapsed: <strong>{formatTime(timeElapsed)}</strong>
              </Text>
            </Box>
            <Progress
              value={(timeElapsed / 120) * 100}
              size="sm"
              colorScheme={timeElapsed > 90 ? "red" : "blue"}
              borderRadius="full"
              flex={1}
            />
            <Text fontSize="xs" color="gray.500">
              Timeout in {formatTime(120 - timeElapsed)}
            </Text>
          </HStack>
        </ModalBody>

        <ModalFooter>
          <Button
            colorScheme="blue"
            size="lg"
            leftIcon={<Send size={20} />}
            onClick={handleSubmit}
            isLoading={isSubmitting}
            isDisabled={!response.trim()}
            loadingText="Sending..."
            width="full"
          >
            Send Response
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}; 