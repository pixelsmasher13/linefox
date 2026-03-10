import React, { FC, useState } from "react";
import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
  Button,
  VStack,
  Input,
  Progress,
  Spinner,
  Box,
  Text as ChakraText,
} from "@chakra-ui/react";
import { Text } from "@heelix-app/design";

interface IntakeWizardModalProps {
  isOpen: boolean;
  questions: string[];
  onCancel: () => void;
  onComplete: (answers: { question: string; answer: string }[]) => void | Promise<void>;
}

export const IntakeWizardModal: FC<IntakeWizardModalProps> = ({
  isOpen,
  questions,
  onCancel,
  onComplete,
}) => {
  const [currentIndex, setCurrentIndex] = useState(0);
  const [answers, setAnswers] = useState<{ question: string; answer: string }[]>(
    []
  );
  const [currentAnswer, setCurrentAnswer] = useState("");
  const [isProcessing, setIsProcessing] = useState(false);

  const total = questions.length;
  const currentQuestion = questions[currentIndex] || "";

  const handleNext = async () => {
    if (!currentAnswer.trim()) return;
    const updatedAnswers = [...answers, { question: currentQuestion, answer: currentAnswer.trim() }];
    setAnswers(updatedAnswers);
    setCurrentAnswer("");
    
    if (currentIndex + 1 < total) {
      setCurrentIndex(currentIndex + 1);
    } else {
      setIsProcessing(true);
      await onComplete(updatedAnswers);
      setIsProcessing(false);
    }
  };

  const progressPercent = ((currentIndex) / total) * 100;

  return (
    <Modal isOpen={isOpen} onClose={() => {}} closeOnOverlayClick={false} size="lg">
      <ModalOverlay />
      <ModalContent>
        <ModalHeader>Additional Details Needed</ModalHeader>
        <ModalBody>
          {isProcessing ? (
            <VStack spacing={6} py={8}>
              <Spinner
                thickness="4px"
                speed="0.65s"
                emptyColor="gray.200"
                color="blue.500"
                size="xl"
              />
              <VStack spacing={2} align="center">
                <Text type="l" bold>
                  Processing Your Task
                </Text>
                <Text type="m" secondary>
                  Updating the task with your details...
                </Text>
              </VStack>
            </VStack>
          ) : (
            <VStack spacing={4} align="stretch">
              <Progress value={progressPercent} size="sm" colorScheme="blue" />
              <ChakraText fontWeight="bold">Question {currentIndex + 1} of {total}</ChakraText>
              <ChakraText>{currentQuestion}</ChakraText>
              <Input
                placeholder="Your answer"
                value={currentAnswer}
                onChange={(e) => setCurrentAnswer(e.target.value)}
                autoFocus
                disabled={isProcessing}
              />
            </VStack>
          )}
        </ModalBody>
        <ModalFooter justifyContent="space-between">
          <Button variant="ghost" onClick={onCancel} disabled={isProcessing}>Cancel</Button>
          <Button 
            colorScheme="blue" 
            onClick={handleNext} 
            disabled={isProcessing || !currentAnswer.trim()}
          >
            {currentIndex + 1 === total ? "Finish" : "Next"}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}; 