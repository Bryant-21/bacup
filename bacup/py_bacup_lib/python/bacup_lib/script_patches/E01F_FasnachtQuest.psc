; Stage constants and the ProtectronList rows come from the QUST VMAD. Each marcher's ToParadeStart scene sets its
; MarcherStageMin-Max arrival stage; the stage fragments chain arrival, parade and attack stages.
Event OnStageSet(Int auiStageID, Int auiItemID)
    ; Stage 400 is both the beekeeper's found stage and the honey beast start, so marcher bookkeeping runs on every stage.
    If ProtectronList != None
        HandleMarcherStage(auiStageID)
    EndIf
    If auiStageID >= 1 && auiStageID <= 6
        ; Stages 1-6 are the authored debug switches for MegaSloth, Ogua, Blue Devil, Sheepsquatch, Grafton Monster and Behemoth.
        If BossWaves != None && auiStageID <= BossWaves.Length
            ChosenBossWave = BossWaves[auiStageID - 1].WaveIndex
        EndIf
    ElseIf auiStageID == 100
        ResetRunState()
        SelectMarchers()
    ElseIf auiStageID == MusicianStageMin
        StartTimer(InstrumentTimerDuration, InstrumentTimerID)
    ElseIf auiStageID == 400
        StartWaveByIndex(0)
    ElseIf auiStageID == 5100
        StartWaveByIndex(UseCurrentPatchWaves(7, 1))
    ElseIf auiStageID == 6100
        StartWaveByIndex(UseCurrentPatchWaves(8, 5))
        If !IsStageDone(Wave2StartStage)
            SetStage(Wave2StartStage)
        EndIf
    ElseIf auiStageID == 7100
        StartWaveByIndex(2)
        StartBossWave()
    ElseIf auiStageID >= DecoratorStageMin && auiStageID <= DecoratorStageMax
        DecorationCount += 1
        SetDisplayVariable("Decorations", DecorationCount)
        If DecorationCount >= MaxDecorationCount && !IsStageDone(HelpedDecoratorStage)
            SetStage(HelpedDecoratorStage)
        EndIf
    EndIf
EndEvent

Function HandleMarcherStage(Int aiStage)
    Int index = ProtectronList.FindStruct("SelectedStage", aiStage)
    If index >= 0
        SpawnMarcher(index)
        Return
    EndIf

    index = ProtectronList.FindStruct("FoundStage", aiStage)
    If index >= 0
        ProtectronCount += 1
        SetDisplayVariable("ProtectronsFound", ProtectronCount)
        If ProtectronCount >= SelectedMarcherTotal() && !IsStageDone(AllProtectronsFound)
            SetStage(AllProtectronsFound)
        EndIf
        Return
    EndIf

    index = ProtectronList.FindStruct("HelpedStage", aiStage)
    If index >= 0
        MarcherHelpedCount += 1
        If MarcherHelpedCount >= SelectedMarcherTotal() && !IsStageDone(AllMarchersHelpedStage)
            SetStage(AllMarchersHelpedStage)
        EndIf
    EndIf
EndFunction

Int Function SelectedMarcherTotal()
    Int total = 0
    Int index = 0
    While index < ProtectronList.Length
        If IsStageDone(ProtectronList[index].SelectedStage)
            total += 1
        EndIf
        index += 1
    EndWhile
    If total <= 0
        Return MarcherCount
    EndIf
    Return total
EndFunction

; MarcherCount of the eight marchers take part; their aliases only exist once spawned.
Function SelectMarchers()
    If ProtectronList == None || ProtectronList.Length == 0
        Return
    EndIf
    Int wanted = MarcherCount
    If wanted > ProtectronList.Length
        wanted = ProtectronList.Length
    EndIf
    Int picked = 0
    Int attempts = 0
    While picked < wanted && attempts < 100
        Int candidate = Utility.RandomInt(0, ProtectronList.Length - 1)
        Int selectedStage = ProtectronList[candidate].SelectedStage
        If !IsStageDone(selectedStage)
            SetStage(selectedStage)
            picked += 1
        EndIf
        attempts += 1
    EndWhile
EndFunction

Function SpawnMarcher(Int aiIndex)
    Marcher entry = ProtectronList[aiIndex]
    If entry.ProtectronAlias == None || entry.MarkerAlias == None || entry.ProtectronActor == None || entry.ProtectronAlias.GetReference() != None
        Return
    EndIf
    ObjectReference startMarker = entry.MarkerAlias.GetReference()
    If startMarker == None
        Return
    EndIf
    Actor marcherActor = startMarker.PlaceActorAtMe(entry.ProtectronActor)
    If marcherActor == None
        Return
    EndIf
    entry.ProtectronAlias.ForceRefTo(marcherActor)
    If AllMarchers != None
        AllMarchers.AddRef(marcherActor)
    EndIf
EndFunction

; The 15 P52 rows in EncounterWaves are the rebalanced copies; the global switches the update off.
Int Function UseCurrentPatchWaves(Int aiCurrentIndex, Int aiOriginalIndex)
    If E01F_Fasnacht_DisablePatch52Updates != None && E01F_Fasnacht_DisablePatch52Updates.GetValue() > 0.0
        Return aiOriginalIndex
    EndIf
    Return aiCurrentIndex
EndFunction

Function StartWaveByIndex(Int aiWaveIndex)
    DefaultQuestEncounterWaveScript waves = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waves != None
        waves.StartEncounterWave(aiWaveIndex)
    EndIf
EndFunction

Function StartBossWave()
    If E01F_Fasnacht_DisablePatch52Updates != None && E01F_Fasnacht_DisablePatch52Updates.GetValue() > 0.0
        StartWaveByIndex(3)
        Return
    EndIf
    If ChosenBossWave < 0 && BossWaves != None
        Int enabledCount = 0
        Int index = 0
        While index < BossWaves.Length
            If BossWaves[index].DisabledVar == None || BossWaves[index].DisabledVar.GetValue() <= 0.0
                enabledCount += 1
            EndIf
            index += 1
        EndWhile
        If enabledCount > 0
            Int pick = Utility.RandomInt(0, enabledCount - 1)
            index = 0
            While index < BossWaves.Length && ChosenBossWave < 0
                If BossWaves[index].DisabledVar == None || BossWaves[index].DisabledVar.GetValue() <= 0.0
                    If pick == 0
                        ChosenBossWave = BossWaves[index].WaveIndex
                    EndIf
                    pick -= 1
                EndIf
                index += 1
            EndWhile
        EndIf
    EndIf
    If ChosenBossWave < 0
        ChosenBossWave = 3
    EndIf
    StartWaveByIndex(ChosenBossWave)
EndFunction

; Called by the MarcherStageMin-MarcherStageMax arrival fragments.
Function MarcherArrived()
    MarcherReadyCount += 1
    If MarcherReadyCount >= SelectedMarcherTotal() && !IsStageDone(StartParadeStage)
        SetStage(StartParadeStage)
    EndIf
EndFunction

; Quest script variables survive Stop/Start, and Tales re-runs the event.
Function ResetRunState()
    DecorationCount = 0
    ProtectronCount = 0
    MarcherHelpedCount = 0
    MarcherReadyCount = 0
    JamCurrentProgress = 0.0
    ChosenBossWave = -1
    Quest masterOfCeremonies = MasterOfCeremoniesDialogue()
    If masterOfCeremonies != None && !masterOfCeremonies.IsRunning()
        masterOfCeremonies.Start()
    EndIf
EndFunction

; The Master of Ceremonies' barks (Dialogue_E01F_Fasnacht_MasterCeremonies) had a server start.
Quest Function MasterOfCeremoniesDialogue()
    Return Game.GetFormFromFile(0x3EEAA7, "SeventySix.esm") as Quest
EndFunction

Event OnQuestShutdown()
    Quest masterOfCeremonies = MasterOfCeremoniesDialogue()
    If masterOfCeremonies != None && masterOfCeremonies.IsRunning()
        masterOfCeremonies.Stop()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != InstrumentTimerID || IsStageDone(MusicianStageMax) || !IsRunning()
        Return
    EndIf
    If PlayerIsPlayingInstrument()
        JamCurrentProgress += JamProgressIncrement
        If JamCurrentProgress >= JamProgressGoal
            SetStage(MusicianStageMax)
            Return
        EndIf
    EndIf
    StartTimer(InstrumentTimerDuration, InstrumentTimerID)
EndEvent

Bool Function PlayerIsPlayingInstrument()
    Actor player = Game.GetPlayer()
    If Alias_Instruments == None || player.GetSitState() < 2
        Return False
    EndIf
    Int index = 0
    While index < Alias_Instruments.GetCount()
        ObjectReference instrument = Alias_Instruments.GetAt(index)
        If instrument != None && instrument.IsFurnitureInUse() && player.GetDistance(instrument) < 150.0
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

; ParadeMarcher1-5 have no fill rule; the parade scene's follow packages and the death stages 8010-8050 need them filled.
Function FillParadeMarchers()
    If ProtectronList == None || ParadeMarcherAliases == None
        Return
    EndIf
    Int slot = 0
    Int index = 0
    While index < ProtectronList.Length && slot < ParadeMarcherAliases.Length
        ReferenceAlias marcherAlias = ProtectronList[index].ProtectronAlias
        If IsStageDone(ProtectronList[index].SelectedStage) && marcherAlias != None && marcherAlias.GetReference() != None
            ParadeMarcherAliases[slot].ForceRefTo(marcherAlias.GetReference())
            slot += 1
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetDisplayVariable(String asName, Float afValue)
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable(asName, afValue)
    EndIf
EndFunction
