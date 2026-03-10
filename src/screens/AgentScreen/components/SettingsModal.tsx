import React from 'react';
import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalFooter,
  ModalBody,
  ModalCloseButton,
  Button,
  Tabs,
  TabList,
  TabPanels,
  Tab,
  TabPanel,
  Box,
} from '@chakra-ui/react';
import { Settings, Send, MessageCircle, Terminal } from 'lucide-react';
import { useGlobalSettings } from '../../../Providers/SettingsProvider';
import { GeneralSettings } from '../../../features/GeneralSettings';
import { TelegramSettings } from '../../../features/TelegramSettings';
import { DiscordSettings } from '../../../features/DiscordSettings';
import { CliToolsSettings } from '../../../features/CliToolsSettings';

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
}) => {
  useGlobalSettings();

  return (
    <Modal isOpen={isOpen} onClose={onClose} size="lg">
      <ModalOverlay />
      <ModalContent>
        <ModalHeader>Settings</ModalHeader>
        <ModalCloseButton />
        <ModalBody>
          <Tabs variant="soft-rounded" colorScheme="blue">
            <TabList mb={4}>
              <Tab gap={2}>
                <Settings size={14} />
                General
              </Tab>
              <Tab gap={2}>
                <Send size={14} />
                Telegram
              </Tab>
              <Tab gap={2}>
                <MessageCircle size={14} />
                Discord
              </Tab>
              <Tab gap={2}>
                <Terminal size={14} />
                Tools
              </Tab>
            </TabList>

            <TabPanels>
              <TabPanel px={0}>
                <Box maxH="450px" overflowY="auto">
                  <GeneralSettings />
                </Box>
              </TabPanel>

              <TabPanel px={0}>
                <Box maxH="450px" overflowY="auto">
                  <TelegramSettings />
                </Box>
              </TabPanel>

              <TabPanel px={0}>
                <Box maxH="450px" overflowY="auto">
                  <DiscordSettings />
                </Box>
              </TabPanel>

              <TabPanel px={0}>
                <Box maxH="450px" overflowY="auto">
                  <CliToolsSettings />
                </Box>
              </TabPanel>
            </TabPanels>
          </Tabs>
        </ModalBody>

        <ModalFooter>
          <Button variant="outline" mr={3} onClick={onClose}>
            Close
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
};
