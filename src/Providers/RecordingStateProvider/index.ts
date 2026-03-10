import { useContext } from 'react';
import { RecordingStateContext, RecordingStateProvider } from './RecordingStateContext';

export const useRecordingState = () => useContext(RecordingStateContext);
export { RecordingStateProvider };
export * from "./types";
export * from "./utils";
