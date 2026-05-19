import React, { useState } from "react";
import {
  Flex,
  Box,
  VStack,
  HStack,
  Text as ChakraText,
  useToast,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
  ModalCloseButton,
  Button,
} from "@chakra-ui/react";
import { Database, Copy, FileText } from "lucide-react";
import { ChatMessage, CompletionMetadata } from "../../chatTypes";
import { MarkdownContent } from "../../../../components/MarkdownContent";

interface CompletionBubbleProps {
  message: ChatMessage;
}

export const CompletionBubble: React.FC<CompletionBubbleProps> = ({ message }) => {
  const meta = message.metadata as CompletionMetadata | undefined;
  const [isModalOpen, setIsModalOpen] = useState(false);
  const isQuickReply = message.type === "quick_reply";
  const toast = useToast();

  const handleCopy = async () => {
    if (!meta?.clipboardContent) return;
    try {
      await navigator.clipboard.writeText(meta.clipboardContent);
      toast({ title: "Copied to clipboard", status: "success", duration: 1500 });
    } catch {
      toast({ title: "Failed to copy", status: "error", duration: 2000 });
    }
  };

  return (
    <Flex justify="flex-start" mb={3}>
      <Box
        bg="white"
        border="1px solid"
        borderColor="gray.200"
        px={isQuickReply ? 4 : 5}
        py={isQuickReply ? 3 : 4}
        borderRadius="xl"
        borderTopLeftRadius="sm"
        maxW={isQuickReply ? "85%" : "95%"}
      >
        <VStack align="stretch" spacing={0}>
          <Box>
            <MarkdownContent content={message.content} />
          </Box>

          {meta?.clipboardContent && (
            <>
              <HStack
                spacing={2}
                py={1}
                px={2}
                borderRadius="md"
                cursor="pointer"
                _hover={{ bg: "gray.50" }}
                onClick={() => setIsModalOpen(true)}
              >
                <Database size={14} color="#6b7280" />
                <ChakraText fontSize="xs" color="gray.500" fontWeight="semibold">
                  Collected Data
                </ChakraText>
              </HStack>

              {/* Full modal */}
              <Modal isOpen={isModalOpen} onClose={() => setIsModalOpen(false)} size="xl">
                <ModalOverlay />
                <ModalContent>
                  <ModalHeader>
                    <HStack spacing={2}>
                      <FileText size={20} />
                      <ChakraText fontWeight="bold">Collected Data</ChakraText>
                    </HStack>
                  </ModalHeader>
                  <ModalCloseButton />
                  <ModalBody>
                    <Box
                      p={4}
                      bg="gray.50"
                      borderRadius="md"
                      maxH="400px"
                      overflowY="auto"
                    >
                      <ChakraText fontSize="sm" whiteSpace="pre-wrap" fontFamily="mono">
                        {meta.clipboardContent}
                      </ChakraText>
                    </Box>
                  </ModalBody>
                  <ModalFooter>
                    <Button variant="ghost" mr={3} onClick={() => setIsModalOpen(false)}>
                      Close
                    </Button>
                    <Button colorScheme="blue" leftIcon={<Copy size={16} />} onClick={() => handleCopy()}>
                      Copy to Clipboard
                    </Button>
                  </ModalFooter>
                </ModalContent>
              </Modal>
            </>
          )}
        </VStack>
      </Box>
    </Flex>
  );
};
