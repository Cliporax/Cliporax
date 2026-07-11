import React, { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Combobox, createCombobox } from "../components/Combobox";

const options = [
  { value: "text", label: "Text" },
  { value: "image", label: "Image" },
  { value: "file", label: "File" },
];

describe("Combobox", () => {
  it("selects an option without rendering a native select", () => {
    function ControlledCombobox() {
      const [value, setValue] = useState("text");
      return <Combobox ariaLabel="Clipboard type" options={options} value={value} onChange={setValue} />;
    }
    const { container } = render(<ControlledCombobox />);

    fireEvent.click(screen.getByRole("combobox", { name: "Clipboard type" }));
    fireEvent.click(screen.getByRole("option", { name: "Image" }));

    expect(screen.getByRole("combobox", { name: "Clipboard type" }).textContent).toContain("Image");
    expect(container.querySelector("select")).toBeNull();
  });

  it("filters searchable options and supports plugin DOM consumers", () => {
    const onChange = vi.fn();
    const instance = createCombobox({ options, searchable: true, onChange, theme: "light" });
    document.body.append(instance.element);

    fireEvent.click(instance.element.querySelector('[role="combobox"]')!);
    fireEvent.input(screen.getByRole("textbox", { name: "Search options" }), { target: { value: "fi" } });
    fireEvent.click(screen.getByRole("option", { name: "File" }));

    expect(onChange).toHaveBeenCalledWith("file");
    instance.destroy();
    instance.element.remove();
  });
});
