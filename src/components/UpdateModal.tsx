import React, { useEffect, useState } from "react";
import {
  Box,
  Button,
  VStack,
  HStack,
  Text as ChakraText,
  useToast,
  Spinner,
  IconButton,
  Slide,
} from "@chakra-ui/react";
import { Text } from "@heelix-app/design";
import { Download, RefreshCw, X } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

interface UpdateInfo {
  version: string;
  date?: string;
  body?: string;
  mandatory?: boolean;
}

export const UpdateModal: React.FC = () => {
  const [isOpen, setIsOpen] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [isInstalling, setIsInstalling] = useState(false);
  const toast = useToast();

  const isMandatory = updateInfo?.mandatory === true;

  useEffect(() => {
    // Listen for update-available event from Rust
    const unlisten = listen<UpdateInfo>("update-available", (event) => {
      console.log("Update available:", event.payload);
      setUpdateInfo(event.payload);
      setIsOpen(true);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleInstallUpdate = async () => {
    setIsInstalling(true);
    try {
      await invoke("install_update");
      toast({
        title: "Update installed",
        description: "The app will restart to complete the update",
        status: "success",
        duration: 3000,
        isClosable: true,
      });
      // App will restart automatically
    } catch (error) {
      console.error("Failed to install update:", error);
      toast({
        title: "Update failed",
        description: error?.toString() || "Failed to install update",
        status: "error",
        duration: 5000,
        isClosable: true,
      });
      setIsInstalling(false);
    }
  };

  const handleClose = () => {
    setIsOpen(false);
    setUpdateInfo(null);
  };

  if (!updateInfo) return null;

  return (
    <Slide direction="bottom" in={isOpen} style={{ zIndex: 9999 }}>
      <Box
        position="fixed"
        bottom="20px"
        right="20px"
        width="380px"
        bg="white"
        borderRadius="12px"
        boxShadow="0 8px 32px rgba(0, 0, 0, 0.12), 0 2px 8px rgba(0, 0, 0, 0.08)"
        border="1px solid"
        borderColor="gray.200"
        overflow="hidden"
      >
        {/* Header */}
        <Box
          bg={isMandatory ? "orange.50" : "blue.50"}
          borderBottom="1px solid"
          borderColor={isMandatory ? "orange.200" : "blue.200"}
          px={4}
          py={3}
        >
          <HStack justify="space-between" align="start">
            <HStack spacing={2}>
              <RefreshCw size={18} color={isMandatory ? "#C05621" : "#2563EB"} />
              <VStack align="start" spacing={0}>
                <ChakraText fontSize="sm" fontWeight="bold" color={isMandatory ? "orange.800" : "blue.800"}>
                  {isMandatory ? "Critical Update" : "Update Available"}
                </ChakraText>
                <ChakraText fontSize="xs" color={isMandatory ? "orange.600" : "blue.600"}>
                  Version {updateInfo.version}
                </ChakraText>
              </VStack>
            </HStack>
            {!isMandatory && (
              <IconButton
                aria-label="Close"
                icon={<X size={16} />}
                size="xs"
                variant="ghost"
                onClick={handleClose}
                isDisabled={isInstalling}
              />
            )}
          </HStack>
        </Box>

        {/* Body */}
        <VStack spacing={3} align="stretch" p={4}>
          {isMandatory && (
            <HStack spacing={2} bg="orange.50" p={2} borderRadius="md" border="1px solid" borderColor="orange.200">
              <ChakraText fontSize="xs" color="orange.700">
                ⚠️ Required to continue using the app
              </ChakraText>
            </HStack>
          )}

          {updateInfo.body && (
            <Box>
              <ChakraText fontSize="xs" fontWeight="bold" color="gray.700" mb={1}>
                What's New:
              </ChakraText>
              <ChakraText fontSize="xs" color="gray.600" noOfLines={3}>
                {updateInfo.body}
              </ChakraText>
            </Box>
          )}

          {/* Actions */}
          <HStack spacing={2}>
            {!isMandatory && (
              <Button
                size="sm"
                variant="ghost"
                onClick={handleClose}
                isDisabled={isInstalling}
                flex={1}
              >
                Later
              </Button>
            )}
            <Button
              size="sm"
              colorScheme={isMandatory ? "orange" : "blue"}
              onClick={handleInstallUpdate}
              leftIcon={isInstalling ? <Spinner size="xs" /> : <Download size={14} />}
              isLoading={isInstalling}
              loadingText="Installing..."
              flex={1}
            >
              {isInstalling ? "Installing..." : "Update"}
            </Button>
          </HStack>
        </VStack>
      </Box>
    </Slide>
  );
};
