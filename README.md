# ARAI

ARAI is a voice-first prompt and writing assistant designed to improve human workflows with AI agents.

Instead of typing long instructions, an operator can speak naturally, include rich context, and quickly produce a clean, structured output that is ready to use.

## Why ARAI exists

Typing can be slow when you need to provide detailed context to an AI system. ARAI was built to make that process faster and easier by turning spoken thoughts into high-quality text.

Its primary goal is to improve workflows where humans collaborate with agents, but it is not limited to agent prompting.

## How it works

ARAI uses a two-stage pipeline:

1. **Speech → text transcription**
   - Audio is captured from the microphone.
   - A local speech model (or configured model path) transcribes speech into text.

2. **Text → polished output transformation**
   - The transcribed text is sent through an LLM transformation step.
   - The model cleans up, formats, and reshapes the text according to your intent.

The result is output that is ready to copy and use in your target workflow.

## What you can use it for

- Prompting AI agents with richer context
- Drafting formal emails
- Writing messages
- Creating document drafts
- General speech-to-text enhancement workflows

## Customizable prompt behavior

ARAI supports configurable prompt instructions.

That means you can control the style and target of each transformation, for example:

- Formal business email
- Engineering prompt for an autonomous coding agent
- Team status update
- Any other custom output format

You speak once, and ARAI adapts the transformed output to your selected style.

## Summary

ARAI is a practical workflow accelerator:

- speak instead of type,
- preserve context more naturally,
- and produce clean, usable text faster.

It is especially useful for AI-assisted work, but flexible enough for any voice-to-text refinement task.
