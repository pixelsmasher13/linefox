import { type FC, PropsWithChildren } from "react";
import styled from "styled-components";

const Container = styled.div`
  display: grid;
  height: 100vh;
  overflow: hidden;
  grid-template-columns: 350px 1fr;
  grid-template-rows: 56px 1fr;
  grid-template-areas:
    "sidebar header"
    "sidebar content";
  background: var(--card-content-background);
  position: relative;
  font-weight: normal;
`;

export const ScreenContainer: FC<PropsWithChildren> = ({ children }) => (
  <Container>{children}</Container>
);
