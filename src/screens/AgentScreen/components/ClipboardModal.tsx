import React, { useState } from 'react';
import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalFooter,
  ModalBody,
  ModalCloseButton,
  Button,
  Box,
  Textarea,
  useToast,
  IconButton,
  Tooltip,
  HStack,
} from '@chakra-ui/react';
import { FileText, Copy } from 'lucide-react';
import { Text } from '@heelix-app/design';

interface ClipboardModalProps {
  clipboardContent: string | null;
  onClose: () => void;
}

export const ClipboardModal: React.FC<ClipboardModalProps> = ({
  clipboardContent,
  onClose,
}) => {
  const [isModalOpen, setIsModalOpen] = useState(false);
  const toast = useToast();

  const handleCopy = async () => {
    if (!clipboardContent) return;

    try {
      await navigator.clipboard.writeText(clipboardContent);
      toast({
        title: 'Copied to clipboard',
        status: 'success',
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      console.error('Failed to copy:', error);
      toast({
        title: 'Failed to copy',
        status: 'error',
        duration: 3000,
        isClosable: true,
      });
    }
  };

  if (!clipboardContent) return null;

  return (
    <>
      <Tooltip label="View collected data" placement="top">
        <IconButton
          aria-label="View clipboard"
          icon={<FileText size={20} />}
          onClick={() => setIsModalOpen(true)}
          colorScheme="blue"
          variant="outline"
          size="sm"
        />
      </Tooltip>

      <Modal isOpen={isModalOpen} onClose={() => setIsModalOpen(false)} size="xl">
        <ModalOverlay />
        <ModalContent>
          <ModalHeader>
            <HStack spacing={2}>
              <FileText size={24} />
              <Text type="l" bold>
                Collected Data
              </Text>
            </HStack>
          </ModalHeader>
          <ModalCloseButton />
          <ModalBody>
            <Box>
              <Box mb={2}>
                <Text type="s" secondary>
                  Data collected during agent run:
                </Text>
              </Box>
              <Textarea
                value={clipboardContent}
                readOnly
                fontFamily="mono"
                minHeight="300px"
                maxHeight="500px"
                resize="vertical"
                bg="gray.50"
                _dark={{ bg: 'gray.800' }}
              />
            </Box>
          </ModalBody>

          <ModalFooter>
            <Button variant="ghost" mr={3} onClick={() => setIsModalOpen(false)}>
              Close
            </Button>
            <Button
              colorScheme="blue"
              leftIcon={<Copy size={16} />}
              onClick={handleCopy}
            >
              Copy to Clipboard
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    </>
  );
};

export default ClipboardModal;