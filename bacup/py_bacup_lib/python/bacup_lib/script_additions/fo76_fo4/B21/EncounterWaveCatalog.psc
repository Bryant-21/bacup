Scriptname B21:EncounterWaveCatalog Extends Quest Const
{Spawn candidates for converted FO76 encounter waves.

FO76 resolves each DefaultQuestEncounterWaveScript wave to WAVE records through
its EncounterTypeKeyword or StoredEncounterWave. FO4 has no WAVE record, so the
converter copies every entry here. Rows are parallel: row i belongs to wave
CandidateWaveIndices[i] (an EncounterWaves index), WAVE variant
CandidateVariants[i] (one per matching WAVE record, numbered from 0 within the
wave), spawn slot CandidateSpawnSlots[i] (the WAVE entry spawn order) and spawns
CandidateForms[i].

Encounters that choose their creature at runtime (wandering boss, horde,
swarm) also carry keyword-pool rows: wave index -1, CandidateKeywords[i] the
encounter keyword, and variants numbered per keyword.}

Int[] Property CandidateWaveIndices Auto Const
Int[] Property CandidateVariants Auto Const
Int[] Property CandidateSpawnSlots Auto Const
Form[] Property CandidateForms Auto Const
Keyword[] Property CandidateKeywords Auto Const

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

Bool Function KeywordPoolIsValid()
    Return IsValid() && CandidateKeywords != None && CandidateKeywords.Length == CandidateForms.Length
EndFunction

Bool Function IsPoolRow(Int aiRow, Keyword akKeyword)
    Return CandidateWaveIndices[aiRow] < 0 && CandidateKeywords[aiRow] == akKeyword && CandidateForms[aiRow] != None
EndFunction

Int Function KeywordVariantCount(Keyword akKeyword)
    If akKeyword == None || !KeywordPoolIsValid()
        Return 0
    EndIf
    Int variants = 0
    Int row = 0
    While row < CandidateForms.Length
        If IsPoolRow(row, akKeyword) && CandidateVariants[row] >= variants
            variants = CandidateVariants[row] + 1
        EndIf
        row += 1
    EndWhile
    Return variants
EndFunction

Form Function PickKeywordCandidate(Keyword akKeyword, Int aiVariant, Int aiActorIndex)
    If akKeyword == None || !KeywordPoolIsValid()
        Return None
    EndIf
    Int slots = 0
    Int row = 0
    While row < CandidateForms.Length
        If IsPoolRow(row, akKeyword) && CandidateVariants[row] == aiVariant && CandidateSpawnSlots[row] >= slots
            slots = CandidateSpawnSlots[row] + 1
        EndIf
        row += 1
    EndWhile
    If slots <= 0
        Return None
    EndIf
    Int slot = aiActorIndex % slots
    Int matches = 0
    row = 0
    While row < CandidateForms.Length
        If IsPoolRow(row, akKeyword) && CandidateVariants[row] == aiVariant && CandidateSpawnSlots[row] == slot
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
        If IsPoolRow(row, akKeyword) && CandidateVariants[row] == aiVariant && CandidateSpawnSlots[row] == slot
            If chosen == 0
                Return CandidateForms[row]
            EndIf
            chosen -= 1
        EndIf
        row += 1
    EndWhile
    Return None
EndFunction

; A uniformly random pool keyword, so an encounter with no usable hint still picks a creature.
Keyword Function RandomPoolKeyword()
    If !KeywordPoolIsValid()
        Return None
    EndIf
    Keyword[] distinct = New Keyword[0]
    Int row = 0
    While row < CandidateKeywords.Length
        Keyword candidate = CandidateKeywords[row]
        If CandidateWaveIndices[row] < 0 && candidate != None && distinct.Find(candidate) < 0 && distinct.Length < 128
            distinct.Add(candidate)
        EndIf
        row += 1
    EndWhile
    If distinct.Length == 0
        Return None
    EndIf
    Return distinct[Utility.RandomInt(0, distinct.Length - 1)]
EndFunction

; An XMarker on walkable navmesh about afDistance from akCenter, for roaming encounter anchors and spawn points.
ObjectReference Function PlaceRoamingMarker(ObjectReference akCenter, Float afDistance) Global
    Form xMarker = Game.GetFormFromFile(0x0000003B, "Fallout4.esm")
    If akCenter == None || xMarker == None
        Return None
    EndIf
    ObjectReference marker = akCenter.PlaceAtMe(xMarker, 1, False, False, False)
    If marker != None && afDistance > 0.0
        Float angle = Utility.RandomFloat(0.0, 360.0)
        marker.MoveTo(akCenter, afDistance * Math.Cos(angle), afDistance * Math.Sin(angle), 0.0, True)
        marker.MoveToNearestNavmeshLocation()
    EndIf
    Return marker
EndFunction
