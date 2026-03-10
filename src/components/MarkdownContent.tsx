import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Box } from "@chakra-ui/react";

interface MarkdownContentProps {
  content: string;
}

export const MarkdownContent = ({ content }: MarkdownContentProps) => (
  <Box
    fontSize="sm"
    color="gray.800"
    lineHeight="1.6"
    fontFamily="body"
    sx={{
      "h1, h2, h3, h4": { fontWeight: "bold", mt: 3, mb: 1 },
      h1: { fontSize: "lg" },
      h2: { fontSize: "md" },
      h3: { fontSize: "sm" },
      p: { mb: 2 },
      "ul, ol": { pl: 5, mb: 2 },
      li: { mb: 1 },
      "code": { bg: "gray.100", px: "4px", borderRadius: "4px", fontSize: "xs", fontFamily: "mono" },
      pre: { bg: "gray.50", border: "1px solid", borderColor: "gray.200", borderRadius: "md", p: 3, mb: 2, overflowX: "auto", fontSize: "xs", fontFamily: "mono" },
      "pre code": { bg: "transparent", p: 0 },
      blockquote: { borderLeft: "3px solid", borderColor: "blue.300", pl: 3, mb: 2, color: "gray.600", fontStyle: "italic" },
      hr: { my: 3, borderColor: "gray.200" },
      a: { color: "blue.500", textDecoration: "underline" },
      strong: { fontWeight: "bold" },
      em: { fontStyle: "italic" },
    }}
  >
    <ReactMarkdown remarkPlugins={[remarkGfm]}>{content}</ReactMarkdown>
  </Box>
);
