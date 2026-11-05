import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, expect, it } from "vitest";
import {
  setAdvancedInterface,
  useAdvancedInterface,
} from "@/lib/interfaceMode";

function Mode() {
  const advanced = useAdvancedInterface();
  return (
    <button onClick={() => setAdvancedInterface(!advanced)}>
      {advanced ? "高级" : "简洁"}
    </button>
  );
}

beforeEach(() => localStorage.clear());

it("defaults to the simple interface and persists a user's explicit choice", () => {
  const first = render(<Mode />);
  fireEvent.click(screen.getByRole("button", { name: "简洁" }));
  expect(screen.getByRole("button", { name: "高级" })).toBeInTheDocument();
  first.unmount();
  render(<Mode />);
  expect(screen.getByRole("button", { name: "高级" })).toBeInTheDocument();
});
