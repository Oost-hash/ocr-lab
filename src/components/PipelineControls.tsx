interface Props {
  preprocess: boolean;
  onPreprocessChange: (value: boolean) => void;
}

export default function PipelineControls({
  preprocess,
  onPreprocessChange,
}: Props) {
  return (
    <div className="pipeline-controls">
      <label className="control">
        <input
          type="checkbox"
          checked={preprocess}
          onChange={(e) => onPreprocessChange(e.target.checked)}
        />
        <span>Preprocess (grayscale + contrast)</span>
      </label>
    </div>
  );
}
