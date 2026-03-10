import React, { useEffect, useState } from "react";
import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalBody,
  Spinner,
  VStack,
  Progress,
  Box,
} from "@chakra-ui/react";
import { Text } from "@heelix-app/design";

interface RecordingProcessingModalProps {
  isOpen: boolean;
}

const processingSteps = [
  {
    title: "Analyzing Your Actions",
    description: "Reviewing the sequence of actions you performed...",
    duration: 3000,
  },
  {
    title: "Creating Action Script",
    description: "Converting your actions into a reusable task script...",
    duration: 3000,
  },
  {
    title: "Identifying Key Elements",
    description: "Finding the important UI elements and patterns...",
    duration: 3000,
  },
  {
    title: "Generating Smart Questions",
    description: "Creating questions to make your task more flexible...",
    duration: 3000,
  },
  {
    title: "Finalizing Your Task",
    description: "Almost ready! Putting the finishing touches...",
    duration: 2000,
  },
];

export const RecordingProcessingModal: React.FC<RecordingProcessingModalProps> = ({ isOpen }) => {
  const [currentStep, setCurrentStep] = useState(0);
  const [progress, setProgress] = useState(0);

  useEffect(() => {
    if (!isOpen) {
      // Reset when modal closes
      setCurrentStep(0);
      setProgress(0);
      return;
    }

    // Start the progress animation
    const progressInterval = setInterval(() => {
      setProgress((prev) => {
        const newProgress = prev + 2;
        return newProgress > 90 ? 90 : newProgress; // Cap at 90% until actually done
      });
    }, 300);

    // Cycle through messages
    const stepInterval = setInterval(() => {
      setCurrentStep((prev) => {
        const nextStep = prev + 1;
        return nextStep >= processingSteps.length ? 0 : nextStep;
      });
    }, processingSteps[currentStep].duration);

    return () => {
      clearInterval(progressInterval);
      clearInterval(stepInterval);
    };
  }, [isOpen, currentStep]);

  const currentMessage = processingSteps[currentStep];

  return (
    <Modal
      isOpen={isOpen}
      onClose={() => {}} // Prevent closing by clicking outside
      closeOnOverlayClick={false}
      closeOnEsc={false}
      isCentered
      size="md"
    >
      <ModalOverlay />
      <ModalContent>
        <ModalBody py={8} px={6}>
          <VStack spacing={6}>
            <Spinner
              thickness="4px"
              speed="0.65s"
              emptyColor="gray.200"
              color="blue.500"
              size="xl"
            />
            
            <VStack spacing={3} width="full" align="center">
              <Text type="l" bold>
                {currentMessage.title}
              </Text>
              <Text type="m" secondary>
                {currentMessage.description}
              </Text>
              
              <Box width="full" mt={4}>
                <Progress
                  value={progress}
                  size="xs"
                  colorScheme="blue"
                  borderRadius="full"
                  hasStripe
                  isAnimated
                />
              </Box>
              
              <Box mt={2}>
                <Text type="xs" secondary>
                  This typically takes 15-30 seconds
                </Text>
              </Box>
            </VStack>
          </VStack>
        </ModalBody>
      </ModalContent>
    </Modal>
  );
};