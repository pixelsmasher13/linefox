import React from "react";
import { Flex, Box, Text as ChakraText } from "@chakra-ui/react";
import { ChatMessage } from "../../chatTypes";

interface UserMessageBubbleProps {
  message: ChatMessage;
}

export const UserMessageBubble: React.FC<UserMessageBubbleProps> = ({ message }) => {
  return (
    <Flex justify="flex-end" mb={3}>
      <Box
        bg="blue.500"
        color="white"
        px={4}
        py={3}
        borderRadius="xl"
        borderTopRightRadius="sm"
        maxW="85%"
      >
        <ChakraText fontSize="sm" whiteSpace="pre-wrap" lineHeight="1.5">
          {message.content}
        </ChakraText>
      </Box>
    </Flex>
  );
};
