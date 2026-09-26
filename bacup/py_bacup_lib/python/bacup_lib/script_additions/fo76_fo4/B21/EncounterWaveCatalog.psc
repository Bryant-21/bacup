Scriptname B21:EncounterWaveCatalog Extends Quest Const
{Spawn candidates for converted FO76 encounter waves.

FO76 resolves each DefaultQuestEncounterWaveScript wave to WAVE records through
its EncounterTypeKeyword or StoredEncounterWave. FO4 has no WAVE record, so the
converter copies every entry here. Rows are parallel: row i belongs to wave
CandidateWaveIndices[i] (an EncounterWaves index), WAVE variant
CandidateVariants[i] (one per matching WAVE record, numbered from 0 within the
wave), spawn slot CandidateSpawnSlots[i] (the WAVE entry spawn order) and spawns
CandidateForms[i].}

Int[] Property CandidateWaveIndices Auto Const
Int[] Property CandidateVariants Auto Const
Int[] Property CandidateSpawnSlots Auto Const
Form[] Property CandidateForms Auto Const

Bool Function IsValid()
    If CandidateWaveIndices == None || CandidateVariants == None || CandidateSpawnSlots == None || CandidateForms == None
        Return False
    EndIf
    Int rows = CandidateForms.Length
    Return CandidateWaveIndices.Length == rows && CandidateVariants.Length == rows && CandidateSpawnSlots.Length == rows
EndFunction

Int Function VariantCount(Int aiWave)
    If !IsValid()
        Return 0
    EndIf
    Int variants = 0
    Int row = 0
    While row < CandidateForms.Length
        If CandidateWaveIndices[row] == aiWave && CandidateForms[row] != None && CandidateVariants[row] >= variants
            variants = CandidateVariants[row] + 1
        EndIf
        row += 1
    EndWhile
    Return variants
EndFunction

Int Function SlotCount(Int aiWave, Int aiVariant)
    If !IsValid()
        Return 0
    EndIf
    Int slots = 0
    Int row = 0
    While row < CandidateForms.Length
        If CandidateWaveIndices[row] == aiWave && CandidateVariants[row] == aiVariant && CandidateForms[row] != None && CandidateSpawnSlots[row] >= slots
            slots = CandidateSpawnSlots[row] + 1
        EndIf
        row += 1
    EndWhile
    Return slots
EndFunction

Form Function PickCandidate(Int aiWave, Int aiVariant, Int aiActorIndex)
    Int slots = SlotCount(aiWave, aiVariant)
    If slots <= 0
        Return None
    EndIf
    Int slot = aiActorIndex % slots
    Int matches = 0
    Int row = 0
    While row < CandidateForms.Length
        If CandidateWaveIndices[row] == aiWave && CandidateVariants[row] == aiVariant && CandidateSpawnSlots[row] == slot && CandidateForms[row] != None
            matches += 1
        EndIf
        row += 1
    EndWhile
    If matches <= 0
        Return None
    EndIf

    Int chosen = Utility.RandomInt(0, matches - 1)
    row = 0
    While row < CandidateForms.Length
        If CandidateWaveIndices[row] == aiWave && CandidateVariants[row] == aiVariant && CandidateSpawnSlots[row] == slot && CandidateForms[row] != None
            If chosen == 0
                Return CandidateForms[row]
            EndIf
            chosen -= 1
        EndIf
        row += 1
    EndWhile
    Return None
EndFunction
