import React from "react";
import { Flex, Text as ChakraText } from "@chakra-ui/react";
import { ChatMessage } from "../../chatTypes";

interface SystemMessageBubbleProps {
  message: ChatMessage;
}

export const SystemMessageBubble: React.FC<SystemMessageBubbleProps> = ({ message }) => {
  return (
    <Flex justify="center" mb={3}>
      <ChakraText
        fontSize="xs"
        color="gray.500"
        bg="gray.100"
        px={3}
        py={1}
        borderRadius="full"
      >
        {message.content}
      </ChakraText>
    </Flex>
  );
};
