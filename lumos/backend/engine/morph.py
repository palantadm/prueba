"""Morph System - quick fixture substitution.

Allows replacing one or more fixture instances with a different model,
remapping channels intelligently by group (intensity→intensity, pan→pan, etc.)
"""
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple
from .fixture import FixtureDefinition, FixtureInstance, PatchManager, ChannelGroup


@dataclass
class ChannelMapping:
    """Maps a source channel to a target channel."""
    source_channel: str
    target_channel: str
    source_group: ChannelGroup
    target_group: ChannelGroup
    confidence: float  # 0-1, how good this mapping is


@dataclass
class MorphPreview:
    """Preview of what a morph operation will do."""
    source_fixture_ids: List[str]
    target_definition_id: str
    target_mode: str
    channel_mappings: List[ChannelMapping]
    warnings: List[str]
    unmapped_source: List[str]
    unmapped_target: List[str]

    @property
    def coverage(self) -> float:
        total = len(self.unmapped_source) + len(self.channel_mappings)
        if total == 0:
            return 1.0
        return len(self.channel_mappings) / total


@dataclass
class MorphResult:
    """Result of executing a morph operation."""
    original_fixture_ids: List[str]
    new_fixture_ids: List[str]
    channel_mappings: List[ChannelMapping]
    cue_updates: int  # number of cue entries updated


class MorphEngine:
    """Engine for fixture morphing/substitution."""

    # Channel groups that should map 1:1
    PRIORITY_GROUPS = [
        ChannelGroup.INTENSITY,
        ChannelGroup.PAN,
        ChannelGroup.TILT,
        ChannelGroup.COLOR,
        ChannelGroup.SHUTTER,
        ChannelGroup.GOBO,
        ChannelGroup.ZOOM,
        ChannelGroup.FOCUS,
        ChannelGroup.IRIS,
        ChannelGroup.SPEED,
    ]

    def __init__(self, patch: PatchManager):
        self.patch = patch

    def compute_mapping(
        self,
        source_def: FixtureDefinition,
        source_mode: str,
        target_def: FixtureDefinition,
        target_mode: str,
    ) -> List[ChannelMapping]:
        """Compute channel mappings between two fixture types."""
        src_mode = source_def.get_mode(source_mode)
        tgt_mode = target_def.get_mode(target_mode)

        if not src_mode or not tgt_mode:
            return []

        src_channels = src_mode.channels
        tgt_channels = tgt_mode.channels

        mappings: List[ChannelMapping] = []
        used_target: set = set()

        # Group source channels by their group
        src_by_group: Dict[ChannelGroup, List[str]] = {}
        for ch_name in src_channels:
            ch_def = source_def.channels.get(ch_name)
            group = ch_def.group if ch_def else ChannelGroup.OTHER
            src_by_group.setdefault(group, []).append(ch_name)

        # Group target channels by their group
        tgt_by_group: Dict[ChannelGroup, List[str]] = {}
        for ch_name in tgt_channels:
            ch_def = target_def.channels.get(ch_name)
            group = ch_def.group if ch_def else ChannelGroup.OTHER
            tgt_by_group.setdefault(group, []).append(ch_name)

        # Map by group (same group = high confidence)
        for group in self.PRIORITY_GROUPS:
            src_list = src_by_group.get(group, [])
            tgt_list = tgt_by_group.get(group, [])

            for i, src_ch in enumerate(src_list):
                if i < len(tgt_list):
                    tgt_ch = tgt_list[i]
                    if tgt_ch not in used_target:
                        confidence = 1.0 if src_ch.lower() == tgt_ch.lower() else 0.8
                        mappings.append(ChannelMapping(
                            source_channel=src_ch,
                            target_channel=tgt_ch,
                            source_group=group,
                            target_group=group,
                            confidence=confidence,
                        ))
                        used_target.add(tgt_ch)

        # Try name matching for remaining channels
        mapped_src = {m.source_channel for m in mappings}
        for src_ch in src_channels:
            if src_ch in mapped_src:
                continue
            best_match = None
            best_score = 0.0
            for tgt_ch in tgt_channels:
                if tgt_ch in used_target:
                    continue
                score = self._name_similarity(src_ch, tgt_ch)
                if score > best_score and score >= 0.5:
                    best_score = score
                    best_match = tgt_ch

            if best_match:
                src_def = source_def.channels.get(src_ch)
                tgt_def_ch = target_def.channels.get(best_match)
                mappings.append(ChannelMapping(
                    source_channel=src_ch,
                    target_channel=best_match,
                    source_group=src_def.group if src_def else ChannelGroup.OTHER,
                    target_group=tgt_def_ch.group if tgt_def_ch else ChannelGroup.OTHER,
                    confidence=best_score * 0.6,  # lower confidence for name match
                ))
                used_target.add(best_match)

        return mappings

    def _name_similarity(self, a: str, b: str) -> float:
        """Simple word-overlap similarity between channel names."""
        a_words = set(a.lower().replace("_", " ").split())
        b_words = set(b.lower().replace("_", " ").split())
        if not a_words or not b_words:
            return 0.0
        intersection = a_words & b_words
        union = a_words | b_words
        return len(intersection) / len(union)

    def preview_morph(
        self,
        source_fixture_ids: List[str],
        target_definition_id: str,
        target_mode: str = "",
    ) -> Optional[MorphPreview]:
        """Preview a morph without applying it."""
        if not source_fixture_ids:
            return None

        first = self.patch.fixtures.get(source_fixture_ids[0])
        if not first:
            return None

        source_def = self.patch.library.get(first.definition_id)
        target_def = self.patch.library.get(target_definition_id)
        if not source_def or not target_def:
            return None

        source_mode = first.mode
        if not target_mode:
            target_mode = target_def.modes[0].name if target_def.modes else ""

        mappings = self.compute_mapping(source_def, source_mode, target_def, target_mode)

        src_mode_obj = source_def.get_mode(source_mode)
        tgt_mode_obj = target_def.get_mode(target_mode)

        all_src = set(src_mode_obj.channels if src_mode_obj else source_def.channels.keys())
        all_tgt = set(tgt_mode_obj.channels if tgt_mode_obj else target_def.channels.keys())
        mapped_src = {m.source_channel for m in mappings}
        mapped_tgt = {m.target_channel for m in mappings}

        warnings = []
        src_count = len(all_src)
        tgt_count = len(all_tgt)
        if tgt_count < src_count:
            warnings.append(
                f"Target fixture has fewer channels ({tgt_count}) than source ({src_count}). "
                "Some channels will be lost."
            )

        # Check for multiple fixtures with different models
        models = set()
        for fid in source_fixture_ids:
            inst = self.patch.fixtures.get(fid)
            if inst:
                models.add(inst.definition_id)
        if len(models) > 1:
            warnings.append(
                f"Selected {len(source_fixture_ids)} fixtures from {len(models)} different models. "
                "Mapping is based on the first fixture's model."
            )

        return MorphPreview(
            source_fixture_ids=source_fixture_ids,
            target_definition_id=target_definition_id,
            target_mode=target_mode,
            channel_mappings=mappings,
            warnings=warnings,
            unmapped_source=list(all_src - mapped_src),
            unmapped_target=list(all_tgt - mapped_tgt),
        )

    def apply_morph(
        self,
        preview: MorphPreview,
        cue_manager=None,
    ) -> MorphResult:
        """Apply a morph operation: replace source fixtures with new ones.

        Keeps the same DMX addresses. Updates cue data using channel mappings.
        """
        new_ids = []

        for source_id in preview.source_fixture_ids:
            source_inst = self.patch.fixtures.get(source_id)
            if not source_inst:
                continue

            # Create new fixture at same address
            new_inst = self.patch.patch(
                definition_id=preview.target_definition_id,
                name=source_inst.name,
                universe=source_inst.universe,
                address=source_inst.address,
                mode=preview.target_mode,
                group=source_inst.group,
            )
            if new_inst:
                new_inst.position = source_inst.position
                new_inst.notes = source_inst.notes
                new_ids.append(new_inst.id)

        # Remove old fixtures
        for source_id in preview.source_fixture_ids:
            self.patch.unpatch(source_id)

        # Update cues if cue manager provided
        cue_updates = 0
        if cue_manager:
            cue_updates = cue_manager.remap_fixtures(
                old_ids=preview.source_fixture_ids,
                new_ids=new_ids,
                mappings=preview.channel_mappings,
            )

        return MorphResult(
            original_fixture_ids=preview.source_fixture_ids,
            new_fixture_ids=new_ids,
            channel_mappings=preview.channel_mappings,
            cue_updates=cue_updates,
        )
