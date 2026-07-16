import React from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { I18nextProvider } from "react-i18next";
import ContentEditor from "../components/ContentEditor";
import { ThemeProvider } from "../contexts/ThemeContext";
import i18n from "../i18n";

describe("ContentEditor", () => {
  it("does not close when a text selection ends outside the editor", () => {
    const onClose = vi.fn();
    render(
      <ThemeProvider>
        <I18nextProvider i18n={i18n}>
          <ContentEditor
            id={1}
            content="Select this text"
            type="text"
            onClose={onClose}
            onSave={vi.fn()}
          />
        </I18nextProvider>
      </ThemeProvider>,
    );

    const editor = screen.getByTestId("content-editor");
    const textarea = screen.getByTestId("content-editor-textarea");
    fireEvent.pointerDown(textarea, { button: 0 });
    fireEvent.pointerUp(editor, { button: 0 });
    fireEvent.click(editor);

    expect(onClose).not.toHaveBeenCalled();
  });

  it("supports undo and redo through keyboard shortcuts", () => {
    render(
      <ThemeProvider>
        <I18nextProvider i18n={i18n}>
          <ContentEditor
            id={1}
            content="Initial"
            type="text"
            onClose={vi.fn()}
            onSave={vi.fn()}
          />
        </I18nextProvider>
      </ThemeProvider>,
    );

    const textarea = screen.getByTestId("content-editor-textarea");
    fireEvent.change(textarea, { target: { value: "Edited" } });
    fireEvent.keyDown(window, { key: "z", ctrlKey: true });
    expect((textarea as HTMLTextAreaElement).value).toBe("Initial");

    fireEvent.keyDown(window, { key: "z", ctrlKey: true, shiftKey: true });
    expect((textarea as HTMLTextAreaElement).value).toBe("Edited");
  });
});
