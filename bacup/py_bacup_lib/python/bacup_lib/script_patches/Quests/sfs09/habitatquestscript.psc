; Timer IDs: 1 radkelp respawn tick, 2 ending-scene failsafe, 1000+N sludge pile N cooldown.
Event OnQuestInit()
    B21SludgeGathering = False
    ResolveEventScripts()
EndEvent

Event OnQuestShutdown()
    CancelTimer(1)
    CancelTimer(2)
    SetSludgeAvailable(False)
    RemoveRadkelp()
    Quests:sfs09:MoleRatScript moleRats = MoleRatSpawner()
    If moleRats != None
        moleRats.RemoveMoleRats()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        If IsTroughPhaseActive()
            RespawnRadkelp()
            StartRadkelpRespawnTimer()
        EndIf
    ElseIf aiTimerID == 2
        ; The ending scenes set stage 10000 on completion; this covers a scene that cannot play.
        If IsRunning() && !IsStageDone(10000)
            SetStage(10000)
        EndIf
    ElseIf aiTimerID >= 1000 && Alias_Activators_Sludge != None && aiTimerID - 1000 < Alias_Activators_Sludge.GetCount()
        If IsTroughPhaseActive()
            SetSludgePileState(Alias_Activators_Sludge.GetAt(aiTimerID - 1000), 0)
        EndIf
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef != playerRef || B21SludgeGathering || !IsTroughPhaseActive()
        Return
    EndIf
    B21SludgeGathering = True
    Quests:sfs09:SludgeAnimScript sludge = akSender as Quests:sfs09:SludgeAnimScript
    If sludge == None || sludge.CurrState != 0
        B21SludgeGathering = False
        Return
    EndIf
    SetSludgePileState(akSender, 1)
    Int pileIndex = Alias_Activators_Sludge.Find(akSender)
    If pileIndex >= 0 && SludgeCooldown > 0
        StartTimer(SludgeCooldown as Float, 1000 + pileIndex)
    EndIf
    Quests:sfs09:MoleRatScript moleRats = MoleRatSpawner()
    If moleRats != None
        If moleRats.SFS09_Habitat_Sludge != None && GatherYield > 0
            playerRef.AddItem(moleRats.SFS09_Habitat_Sludge, GatherYield, False)
        EndIf
        moleRats.SpawnMoleRats(akSender, GatherMoleRats)
    EndIf
    B21SludgeGathering = False
EndEvent

Function ResolveEventScripts()
    Quest owner = Self as Quest
    EWS = owner as DefaultQuestEncounterWaveScript
    DEQ = owner as DefaultEventQuest
    MoleRatSpawnerScript = Alias_MoleRats as Quests:sfs09:MoleRatScript
    AMS = SFS09_Habitat_Master as Quests:sfs09:arktosmasterscript
EndFunction

DefaultQuestEncounterWaveScript Function WaveScript()
    If EWS == None
        ResolveEventScripts()
    EndIf
    Return EWS
EndFunction

Quests:sfs09:MoleRatScript Function MoleRatSpawner()
    If MoleRatSpawnerScript == None
        ResolveEventScripts()
    EndIf
    Return MoleRatSpawnerScript
EndFunction

B21:QuestTimer Function QuestTimerScript()
    Quest owner = Self as Quest
    Return owner as B21:QuestTimer
EndFunction

Function SetQuestVariable(String asName, Float afValue)
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables != None && asName != ""
        variables.SetVariable(asName, afValue)
    EndIf
EndFunction

Bool Function EventEnded()
    Return IsStageDone(9000) || IsStageDone(9990) || IsStageDone(9991) || IsStageDone(9992)
EndFunction

Bool Function IsTroughPhaseActive()
    Return IsRunning() && IsStageDone(Stage_Phase1Begin) && !IsStageDone(Stage_Phase1End) && !EventEnded()
EndFunction

Bool Function IsHabitatValid(Int aiHabitat)
    Return Habitats != None && aiHabitat >= 0 && aiHabitat < Habitats.Length && Habitats[aiHabitat] != None
EndFunction

Bool Function IsHabitatCreatureAlive(Int aiHabitat)
    Return IsHabitatValid(aiHabitat) && !IsStageDone(Habitats[aiHabitat].CreatureDeadStage)
EndFunction

; Habitat order follows the Habitats property: 0 Hunters (A), 1 Swimmers (C), 2 Scavengers (B).
String Function NextRankVariable(Int aiHabitat)
    If aiHabitat == 0
        Return "VenisonNextRank"
    ElseIf aiHabitat == 1
        Return "RadkelpNextRank"
    ElseIf aiHabitat == 2
        Return "GarbageNextRank"
    EndIf
    Return ""
EndFunction

; Linear rank thresholds; with 10 items for rank 3 they are 4, 7 and 10.
Int Function HabitatTier(Int aiHabitat)
    If !IsHabitatValid(aiHabitat) || MaxTier <= 0
        Return 0
    EndIf
    If NumSpecialItemsNeededForMax <= 0
        Return MaxTier
    EndIf
    Int tier = Habitats[aiHabitat].Count * MaxTier / NumSpecialItemsNeededForMax
    If tier > MaxTier
        tier = MaxTier
    EndIf
    Return tier
EndFunction

Int Function ItemsToNextRank(Int aiHabitat)
    Int tier = HabitatTier(aiHabitat)
    If !IsHabitatValid(aiHabitat) || tier >= MaxTier || NumSpecialItemsNeededForMax <= 0
        Return 0
    EndIf
    Int threshold = ((tier + 1) * NumSpecialItemsNeededForMax + MaxTier - 1) / MaxTier
    Return threshold - Habitats[aiHabitat].Count
EndFunction

Function UpdateHabitatDisplay(Int aiHabitat)
    If !IsHabitatValid(aiHabitat)
        Return
    EndIf
    SetQuestVariable(Habitats[aiHabitat].TextVar_ItemCount, Habitats[aiHabitat].Count as Float)
    SetQuestVariable(Habitats[aiHabitat].TextVar_Tier, HabitatTier(aiHabitat) as Float)
    SetQuestVariable(NextRankVariable(aiHabitat), ItemsToNextRank(aiHabitat) as Float)
EndFunction

Quests:sfs09:TroughScript Function TroughAliasScript(Int aiHabitat)
    Int aliasID = 74
    While aliasID <= 76
        Quests:sfs09:TroughScript trough = GetAlias(aliasID) as Quests:sfs09:TroughScript
        If trough != None && trough.HabitatIndex == aiHabitat
            Return trough
        EndIf
        aliasID += 1
    EndWhile
    Return None
EndFunction

Function SetTroughAnimation(Int aiHabitat, Int aiTier)
    Quests:sfs09:TroughScript trough = TroughAliasScript(aiHabitat)
    If trough == None
        Return
    EndIf
    Quests:sfs09:TroughAnimationScript animation = trough.GetReference() as Quests:sfs09:TroughAnimationScript
    If animation == None
        Return
    EndIf
    If aiTier <= 0
        animation.GoToState("Reset")
    ElseIf aiTier == 1
        animation.GoToState("Stage2")
    ElseIf aiTier == 2
        animation.GoToState("stage3")
    Else
        animation.GoToState("Stage4")
    EndIf
EndFunction

Function SetMainframePowered(Bool abPowered)
    If SFS09_Habitat_MainframePower != None
        SFS09_Habitat_MainframePower.SetValue(abPowered as Float)
    EndIf
    If Alias_Mainframe == None
        Return
    EndIf
    MainframeAnimScript = Alias_Mainframe.GetReference() as Quests:sfs09:MainframeAnimScript
    If MainframeAnimScript == None
        Return
    EndIf
    If abPowered
        MainframeAnimScript.GoToState("On")
    Else
        MainframeAnimScript.GoToState("off")
    EndIf
EndFunction

Function SetWaveEndStage(Int aiWave, Int aiStage)
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves == None || waves.EncounterWaves == None || aiWave < 0 || aiWave >= waves.EncounterWaves.Length
        Return
    EndIf
    waves.EncounterWaves[aiWave].StageToSetAtEnd = aiStage
EndFunction

; Stage 100: a repeat run starts from empty troughs, full power and three living habitats.
Function ResetEvent()
    ResolveEventScripts()
    CancelTimer(1)
    CancelTimer(2)
    B21SludgeGathering = False
    NumAlive = 3
    SetQuestVariable(TextVar_MaxTier, MaxTier as Float)
    SetQuestVariable("NumAlive", 3.0)
    Int index = 0
    While Habitats != None && index < Habitats.Length
        If Habitats[index] != None
            Habitats[index].Count = 0
            UpdateHabitatDisplay(index)
            SetTroughAnimation(index, 0)
            ; The "Stage set by DefaultQuestEncounterWaveScript" wave-cleared stages 310-330 and 410-430.
            SetWaveEndStage(Habitats[index].Wave1, 310 + index * 10)
            SetWaveEndStage(Habitats[index].Wave2, 410 + index * 10)
        EndIf
        index += 1
    EndWhile
    SetMainframePowered(True)
    SetSludgeAvailable(False)
    RemoveRadkelp()
EndFunction

; Stage 150: radstags carry venison, sludge piles can be gathered and radkelp grows at its markers.
Function BeginTroughPhase()
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StartEncounterWave(RadstagWave)
    EndIf
    SetSludgeAvailable(True)
    RespawnRadkelp()
    StartRadkelpRespawnTimer()
    B21:QuestTimer questTimer = QuestTimerScript()
    If questTimer != None
        questTimer.StartQuestTimer()
    EndIf
EndFunction

Function ShutDownMainframe()
    SetMainframePowered(False)
EndFunction

; Stage 200: gathering ends and each habitat attracts the friendly creature its trough rank earned.
Function EndTroughPhase()
    CancelTimer(1)
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StopEncounterWave(RadstagWave, True)
    EndIf
    SetSludgeAvailable(False)
    RemoveRadkelp()
    NumAlive = 3
    SetQuestVariable("NumAlive", 3.0)
    Int index = 0
    While waves != None && Habitats != None && index < Habitats.Length
        If Habitats[index] != None
            Int tier = HabitatTier(index)
            Int friendlyWave = Habitats[index].FriendlyWave0
            If tier == 1
                friendlyWave = Habitats[index].FriendlyWave1
            ElseIf tier == 2
                friendlyWave = Habitats[index].FriendlyWave2
            ElseIf tier >= 3
                friendlyWave = Habitats[index].FriendlyWave3
            EndIf
            waves.StartEncounterWave(friendlyWave)
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetSludgePileState(ObjectReference akPile, Int aiState)
    Quests:sfs09:SludgeAnimScript sludge = akPile as Quests:sfs09:SludgeAnimScript
    If sludge == None
        Return
    EndIf
    sludge.CurrState = aiState
    If aiState == 1
        sludge.GoToState("gathered")
    ElseIf aiState == 0
        sludge.GoToState("available")
    Else
        sludge.GoToState("Unavailable")
    EndIf
EndFunction

Function SetSludgeAvailable(Bool abAvailable)
    Int index = 0
    While Alias_Activators_Sludge != None && index < Alias_Activators_Sludge.GetCount()
        CancelTimer(1000 + index)
        ObjectReference pile = Alias_Activators_Sludge.GetAt(index)
        If pile != None
            If abAvailable
                RegisterForRemoteEvent(pile, "OnActivate")
                SetSludgePileState(pile, 0)
            Else
                UnregisterForRemoteEvent(pile, "OnActivate")
                SetSludgePileState(pile, -1)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function StartRadkelpRespawnTimer()
    If RespawnRate > 0
        StartTimer(RespawnRate as Float, 1)
    EndIf
EndFunction

ObjectReference Function SpawnRadkelp(ObjectReference akMarker)
    If akMarker == None || SFS09_Habitat_Radkelp == None
        Return None
    EndIf
    ObjectReference radkelp = akMarker.PlaceAtMe(SFS09_Habitat_Radkelp, 1, False, False, True)
    If radkelp != None && Alias_RadkelpInLevel != None
        Alias_RadkelpInLevel.AddRef(radkelp)
    EndIf
    Return radkelp
EndFunction

; Every marker keeps one uncollected radkelp; a picked one grows back on the next respawn tick.
Function RespawnRadkelp()
    If Alias_Markers_Radkelp == None || SFS09_Habitat_Radkelp == None
        Return
    EndIf
    If RadkelpMarkers == None
        RadkelpMarkers = New RadkelpMarker[0]
    EndIf
    Int rowIndex = 0
    While rowIndex < RadkelpMarkers.Length
        RadkelpMarker existing = RadkelpMarkers[rowIndex]
        If existing != None && existing.Marker != None
            If existing.Radkelp == None || existing.Radkelp.GetContainer() != None || existing.Radkelp.IsDisabled()
                existing.Radkelp = SpawnRadkelp(existing.Marker)
            EndIf
        EndIf
        rowIndex += 1
    EndWhile
    Int markerIndex = 0
    While markerIndex < Alias_Markers_Radkelp.GetCount() && RadkelpMarkers.Length < MaxRadkelp
        ObjectReference marker = Alias_Markers_Radkelp.GetAt(markerIndex)
        If marker != None && RadkelpMarkers.FindStruct("Marker", marker) < 0
            RadkelpMarker added = New RadkelpMarker
            added.Marker = marker
            added.Radkelp = SpawnRadkelp(marker)
            RadkelpMarkers.Add(added)
        EndIf
        markerIndex += 1
    EndWhile
EndFunction

Function RemoveRadkelp()
    Int rowIndex = 0
    While RadkelpMarkers != None && rowIndex < RadkelpMarkers.Length
        RadkelpMarker row = RadkelpMarkers[rowIndex]
        If row != None && row.Radkelp != None && row.Radkelp.GetContainer() == None
            row.Radkelp.Disable()
            row.Radkelp.Delete()
        EndIf
        rowIndex += 1
    EndWhile
    If Alias_RadkelpInLevel != None
        Alias_RadkelpInLevel.RemoveAll()
    EndIf
    RadkelpMarkers = New RadkelpMarker[0]
EndFunction

Function StartHabitatWaves(Int aiRound)
    DefaultQuestEncounterWaveScript waves = WaveScript()
    Int index = 0
    While waves != None && Habitats != None && index < Habitats.Length
        If IsHabitatCreatureAlive(index)
            If aiRound == 1
                waves.StartEncounterWave(Habitats[index].Wave1)
            Else
                waves.StartEncounterWave(Habitats[index].Wave2)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

; A habitat that clears its round early keeps fighting its endless wave until the round's objective timer ends.
Function StartEndlessWave(Int aiHabitat, Int aiRoundEndStage)
    If EventEnded() || IsStageDone(aiRoundEndStage) || !IsHabitatCreatureAlive(aiHabitat)
        Return
    EndIf
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StartEncounterWave(Habitats[aiHabitat].WaveEndless)
    EndIf
EndFunction

Function StopHabitatWaves(Int aiHabitat)
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves == None || !IsHabitatValid(aiHabitat)
        Return
    EndIf
    waves.StopEncounterWave(Habitats[aiHabitat].Wave1, False)
    waves.StopEncounterWave(Habitats[aiHabitat].Wave2, False)
    waves.StopEncounterWave(Habitats[aiHabitat].WaveEndless, False)
EndFunction

Function EndWaveRound()
    Int index = 0
    While Habitats != None && index < Habitats.Length
        StopHabitatWaves(index)
        index += 1
    EndWhile
    ; Quercus' Formula Q heals the friendly animals between waves once ARIC-4 is shut down.
    If IsStageDone(170)
        HealFriendlyCreatures()
    EndIf
EndFunction

Function HealFriendlyCreatures()
    Bool healed = False
    Int index = 0
    While FriendlyCreatures != None && index < FriendlyCreatures.Length
        Actor creature = None
        If FriendlyCreatures[index] != None
            creature = FriendlyCreatures[index].GetActorReference()
        EndIf
        If creature != None && !creature.IsDead() && SFS09_Habitat_RestoreHealthSpell != None
            SFS09_Habitat_RestoreHealthSpell.Cast(creature, creature)
            healed = True
        EndIf
        index += 1
    EndWhile
    If healed && SFS09_Habitat_Message_Heal != None
        SFS09_Habitat_Message_Heal.Show()
    EndIf
EndFunction

Int Function CountLivingHabitats()
    Int living = 0
    Int index = 0
    While Habitats != None && index < Habitats.Length
        If IsHabitatCreatureAlive(index)
            living += 1
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction

; Stage 500: the alpha attacks one habitat whose friendly creature still lives.
Function ChooseAlphaHabitat()
    Int living = CountLivingHabitats()
    If living <= 0 || EventEnded()
        Return
    EndIf
    B21:QuestTimer questTimer = QuestTimerScript()
    If questTimer != None
        ; The alpha wave's own objective timer decides the outcome, so the event timer must outlast it.
        questTimer.EnsureQuestTimerRemaining(120.0)
    EndIf
    Int pick = Utility.RandomInt(0, living - 1)
    Int index = 0
    While index < Habitats.Length
        If IsHabitatCreatureAlive(index)
            If pick == 0
                If !IsStageDone(Habitats[index].BossStage)
                    SetStage(Habitats[index].BossStage)
                EndIf
                Return
            EndIf
            pick -= 1
        EndIf
        index += 1
    EndWhile
EndFunction

Bool Function BeginAlphaWave(Int aiHabitat)
    If EventEnded() || !IsHabitatValid(aiHabitat)
        Return False
    EndIf
    Int chosen = 0
    Int index = 0
    While index < Habitats.Length
        If Habitats[index] != None && IsStageDone(Habitats[index].BossStage)
            chosen += 1
        EndIf
        index += 1
    EndWhile
    If chosen > 1
        Return False
    EndIf
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StartEncounterWave(Habitats[aiHabitat].WaveBoss)
    EndIf
    Return True
EndFunction

; Stages 910/920/930: a dead friendly creature ends its habitat's waves; losing all three fails the event.
Function FriendlyCreatureDied(Int aiHabitat)
    StopHabitatWaves(aiHabitat)
    NumAlive = CountLivingHabitats()
    SetQuestVariable("NumAlive", NumAlive as Float)
    If NumAlive <= 0 && !EventEnded()
        SetStage(9992)
    EndIf
EndFunction

Function FinishEvent()
    B21:QuestTimer questTimer = QuestTimerScript()
    If questTimer != None
        questTimer.StopQuestTimer()
    EndIf
    CancelTimer(1)
    SetSludgeAvailable(False)
    RemoveRadkelp()
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StopAllEncounterWaves(False)
    EndIf
    StartTimer(60.0, 2)
EndFunction

; Stage 10000: event-only gathering items and spawned mole rats do not outlive the run.
Function CleanUpEvent()
    CancelTimer(1)
    CancelTimer(2)
    SetSludgeAvailable(False)
    RemoveRadkelp()
    Quests:sfs09:MoleRatScript moleRats = MoleRatSpawner()
    If moleRats != None
        moleRats.RemoveMoleRats()
    EndIf
    Actor playerRef = Game.GetPlayer()
    Int index = 0
    While playerRef != None && Habitats != None && index < Habitats.Length
        If Habitats[index] != None && Habitats[index].SpecialItem != None
            Int carried = playerRef.GetItemCount(Habitats[index].SpecialItem)
            If carried > 0
                playerRef.RemoveItem(Habitats[index].SpecialItem, carried, True)
            EndIf
        EndIf
        SetTroughAnimation(index, 0)
        index += 1
    EndWhile
    SetMainframePowered(True)
EndFunction
