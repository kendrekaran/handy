import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { commands } from "@/bindings";

interface ContinuousListeningProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ContinuousListening: React.FC<ContinuousListeningProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, refreshSettings, isUpdating } = useSettings();

    const [updating, setUpdating] = React.useState(false);

    const enabled = getSetting("continuous_listening") ?? false;

    const handleChange = async (value: boolean) => {
      setUpdating(true);
      try {
        const result = await commands.setContinuousListening(value);
        if (result.status === "error") {
          console.error("Failed to toggle continuous listening:", result.error);
        } else {
          await refreshSettings();
        }
      } catch (e) {
        console.error("Failed to toggle continuous listening:", e);
      } finally {
        setUpdating(false);
      }
    };

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={handleChange}
        isUpdating={updating || isUpdating("continuous_listening")}
        label={t("settings.general.continuousListening.label", {
          defaultValue: "Continuous Listening (24/7)",
        })}
        description={t("settings.general.continuousListening.description", {
          defaultValue:
            "Automatically transcribe everything you say without pressing any shortcut. Uses VAD to detect speech.",
        })}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  });
