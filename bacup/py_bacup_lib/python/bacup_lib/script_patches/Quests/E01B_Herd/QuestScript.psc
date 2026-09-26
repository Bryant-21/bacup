Event OnQuestInit()
    EventQuestScript = (Self as Quest) as DefaultEventQuest
    BrahminStillAlive = 3
    currGoal = 0
    StartTimer(2.0, 49860)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 49860
        If IsStopping() || IsStopped() || IsCompleted() || IsStageDone(Stage_ActivityStart) || IsStageDone(205)
            Return
        EndIf
        TryStartActivityForPlayer()
        StartTimer(2.0, 49860)
    ElseIf aiTimerID == 49861
        If IsRunning()
            Stop()
        EndIf
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(49860)
    CancelTimer(49861)
    CleanupEvent()
EndEvent

; FO76 filled EventLoc from the event scheduler and refilled LocRefAliases; FO4
; fills those aliases at start, so an empty EventLoc cannot be recovered here.
Function SetupEvent()
    If Alias_Actor_Handy == None || Alias_Actor_Handy.GetReference() != None
        Return
    EndIf

    Location eventLoc = None
    If Alias_EventLoc != None
        eventLoc = Alias_EventLoc.GetLocation()
    EndIf
    ObjectReference handyMarker = None
    If Alias_Marker_HandySpawn != None
        handyMarker = Alias_Marker_HandySpawn.GetReference()
    EndIf
    If handyMarker == None && EventRoot_Alias != None
        handyMarker = EventRoot_Alias.GetReference()
    EndIf
    If eventLoc == None || handyMarker == None
        Debug.Trace("Quests:E01B_Herd:QuestScript: EventLoc or Marker_HandySpawn is empty; stopping Free Range", 1)
        Stop()
        Return
    EndIf

    FarmDatum farm = None
    Int farmIndex = FindFarmIndex(eventLoc)
    If farmIndex >= 0
        farm = FarmData[farmIndex]
    EndIf

    BrahminStillAlive = 3
    currGoal = 0
    If farm != None && farm.NameLoc != None && Alias_FarmhandName != None
        Alias_FarmhandName.ForceLocationTo(farm.NameLoc)
    EndIf

    Actor playerRef = Game.GetPlayer()
    If playerRef != None && E01B_Herd_Perk != None && !playerRef.HasPerk(E01B_Herd_Perk)
        playerRef.AddPerk(E01B_Herd_Perk)
    EndIf

    SpawnFarmhand(handyMarker, farm)
    SpawnBrahmin(handyMarker, farm)
    TryStartActivityForPlayer()
EndFunction

Int Function FindFarmIndex(Location akLocation)
    If FarmData == None || FarmData.Length == 0
        Return -1
    EndIf
    Int index = 0
    While index < FarmData.Length
        If FarmData[index] != None && FarmData[index].FarmLoc == akLocation
            Return index
        EndIf
        index += 1
    EndWhile
    Return 0
EndFunction

Function SpawnFarmhand(ObjectReference akMarker, FarmDatum akFarm)
    If akMarker == None || E01B_Herd_LvlReaperBot == None
        Return
    EndIf
    Actor handy = akMarker.PlaceActorAtMe(E01B_Herd_LvlReaperBot)
    If handy == None
        Return
    EndIf
    Alias_Actor_Handy.ForceRefTo(handy)
    If ShepherdsCrook != None
        handy.AddItem(ShepherdsCrook, 1, True)
    EndIf
    If akFarm != None && akFarm.BookObj != None
        handy.AddItem(akFarm.BookObj, 1, True)
    EndIf
    handy.KillSilent()
EndFunction

Function SpawnBrahmin(ObjectReference akHandyMarker, FarmDatum akFarm)
    If Brahmin_Aliases == None || akFarm == None || akFarm.Brahmin == None
        Return
    EndIf
    Int index = 0
    While index < Brahmin_Aliases.Length
        ReferenceAlias brahminAlias = Brahmin_Aliases[index]
        If brahminAlias != None && brahminAlias.GetReference() == None
            ObjectReference spawnMarker = None
            If Alias_Markers_Brahmin != None && index < Alias_Markers_Brahmin.GetCount()
                spawnMarker = Alias_Markers_Brahmin.GetAt(index)
            EndIf
            Actor brahmin = None
            If spawnMarker != None
                brahmin = spawnMarker.PlaceActorAtMe(akFarm.Brahmin)
            ElseIf akHandyMarker != None
                spawnMarker = akHandyMarker
                brahmin = akHandyMarker.PlaceActorAtMe(akFarm.Brahmin)
                If brahmin != None
                    Float offset = (BrahminDistanceFromHandy * (index - 1)) as Float
                    brahmin.MoveTo(akHandyMarker, offset, BrahminDistanceFromHandy as Float, 0.0)
                EndIf
            EndIf
            If brahmin != None
                PrepareBrahmin(brahminAlias, brahmin, spawnMarker)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function PrepareBrahmin(ReferenceAlias akAlias, Actor akBrahmin, ObjectReference akSandboxAnchor)
    akAlias.ForceRefTo(akBrahmin)
    If Alias_Actors_Brahmin != None && Alias_Actors_Brahmin.Find(akBrahmin) < 0
        Alias_Actors_Brahmin.AddRef(akBrahmin)
    EndIf
    ; The wiki notes player fire cannot hurt the herd.
    akBrahmin.IgnoreFriendlyHits(True)
    If akSandboxAnchor != None && E01B_Herd_Keyword_StartSandbox != None
        akBrahmin.SetLinkedRef(akSandboxAnchor, E01B_Herd_Keyword_StartSandbox)
    EndIf
    If E01B_Herd_BrahminTravelMarker != None && E01B_Herd_BrahminTravelKeyword != None
        ObjectReference travelMarker = akBrahmin.PlaceAtMe(E01B_Herd_BrahminTravelMarker, 1, True, False, False)
        If travelMarker != None
            akBrahmin.SetLinkedRef(travelMarker, E01B_Herd_BrahminTravelKeyword)
        EndIf
    EndIf
    akBrahmin.EvaluatePackage()
EndFunction

Actor Function GetBrahmin(Int aiIndex)
    If Brahmin_Aliases == None || aiIndex < 0 || aiIndex >= Brahmin_Aliases.Length || Brahmin_Aliases[aiIndex] == None
        Return None
    EndIf
    Return Brahmin_Aliases[aiIndex].GetActorReference()
EndFunction

Int Function CountLivingBrahmin()
    Int living = 0
    Int index = 0
    While Brahmin_Aliases != None && index < Brahmin_Aliases.Length
        Actor brahmin = GetBrahmin(index)
        If brahmin != None && !brahmin.IsDead()
            living += 1
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction

Function TryStartActivityForPlayer()
    If !IsStageDone(100) || IsStageDone(Stage_ActivityStart) || ShepherdsCrook == None
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || playerRef.GetItemCount(ShepherdsCrook) <= 0
        Return
    EndIf
    If EventQuestScript != None && !EventQuestScript.IsPlayerParticipating()
        Return
    EndIf
    SetStage(Stage_ActivityStart)
EndFunction

Function OnPlayerAcquiredCrook()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    If E01B_Herd_Perk != None && !playerRef.HasPerk(E01B_Herd_Perk)
        playerRef.AddPerk(E01B_Herd_Perk)
    EndIf
    ; The equip hint was a Story Manager quest; its node was not converted.
    If E01B_Herd_Equip != None && ShepherdsCrook != None && !playerRef.IsEquipped(ShepherdsCrook) && !E01B_Herd_Equip.IsRunning() && !E01B_Herd_Equip.IsCompleted()
        E01B_Herd_Equip.Start()
    EndIf
EndFunction

ObjectReference Function GoalMarker(Int aiGoal)
    If PathMarkers != None && aiGoal >= 0 && aiGoal < PathMarkers.Length
        If PathMarkers[aiGoal] != None && PathMarkers[aiGoal].GetReference() != None
            Return PathMarkers[aiGoal].GetReference()
        EndIf
    EndIf
    If Alias_Marker_FarmSandbox != None && Alias_Marker_FarmSandbox.GetReference() != None
        Return Alias_Marker_FarmSandbox.GetReference()
    EndIf
    If PathMarkers != None && PathMarkers.Length > 0 && PathMarkers[PathMarkers.Length - 1] != None
        Return PathMarkers[PathMarkers.Length - 1].GetReference()
    EndIf
    Return None
EndFunction

Function MoveTravelMarker(Actor akBrahmin, ObjectReference akDestination)
    If akBrahmin == None || akBrahmin.IsDead() || akDestination == None || E01B_Herd_BrahminTravelKeyword == None
        Return
    EndIf
    ObjectReference travelMarker = akBrahmin.GetLinkedRef(E01B_Herd_BrahminTravelKeyword)
    If travelMarker == None && E01B_Herd_BrahminTravelMarker != None
        travelMarker = akBrahmin.PlaceAtMe(E01B_Herd_BrahminTravelMarker, 1, True, False, False)
        If travelMarker != None
            akBrahmin.SetLinkedRef(travelMarker, E01B_Herd_BrahminTravelKeyword)
        EndIf
    EndIf
    If travelMarker == None
        Return
    EndIf
    travelMarker.MoveTo(akDestination)
    akBrahmin.EvaluatePackage()
EndFunction

; Goals follow PathMarkers: 0 marker 1, 1 scare, 2 marker 2, 3 marker 2.5, 4 marker 3; past the end is the farm sandbox.
Function AdvanceHerdGoal(Int aiGoal)
    currGoal = aiGoal
    ObjectReference destination = GoalMarker(aiGoal)
    Int index = 0
    While Brahmin_Aliases != None && index < Brahmin_Aliases.Length
        MoveTravelMarker(GetBrahmin(index), destination)
        index += 1
    EndWhile
EndFunction

Function ReturnStrayBrahmin(Actor akBrahmin)
    MoveTravelMarker(akBrahmin, GoalMarker(currGoal))
EndFunction

Function CheckHerdCheckpoint(Int aiBrahmin1Stage, Int aiBrahmin2Stage, Int aiBrahmin3Stage, Int aiNextStage)
    If IsStageDone(aiNextStage) || !IsRunning() || IsEventFinished()
        Return
    EndIf
    Bool anyLiving = False
    Int index = 0
    While index < 3
        Actor brahmin = GetBrahmin(index)
        If brahmin != None && !brahmin.IsDead()
            anyLiving = True
            Int reachedStage = aiBrahmin1Stage
            If index == 1
                reachedStage = aiBrahmin2Stage
            ElseIf index == 2
                reachedStage = aiBrahmin3Stage
            EndIf
            If !IsStageDone(reachedStage)
                Return
            EndIf
        EndIf
        index += 1
    EndWhile
    If anyLiving
        SetStage(aiNextStage)
    EndIf
EndFunction

Function ReevaluateHerdCheckpoints()
    If !IsStageDone(205) || IsEventFinished()
        Return
    EndIf
    If !IsStageDone(300)
        CheckHerdCheckpoint(210, 220, 230, 300)
    ElseIf !IsStageDone(305)
        CheckHerdCheckpoint(301, 302, 303, 305)
    ElseIf !IsStageDone(400)
        CheckHerdCheckpoint(310, 320, 330, 400)
    ElseIf !IsStageDone(500)
        CheckHerdCheckpoint(410, 420, 430, 500)
    ElseIf !IsStageDone(600)
        CheckHerdCheckpoint(510, 520, 530, 600)
    EndIf
EndFunction

Bool Function IsEventFinished()
    Return IsStageDone(9000) || IsStageDone(9990) || IsStageDone(9991) || IsStageDone(9992)
EndFunction

Function HandleBrahminDeath(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
    BrahminStillAlive = CountLivingBrahmin()
    If IsEventFinished()
        Return
    EndIf
    If BrahminStillAlive <= 0
        SetStage(9992)
        Return
    EndIf
    ReevaluateHerdCheckpoints()
EndFunction

DefaultQuestEncounterWaveScript Function GetWaveScript()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

; The P56 update added raider rustler variants of the three escort waves; each wave picks one.
Function StartHerdWave(Int aiWaveNumber)
    DefaultQuestEncounterWaveScript waves = GetWaveScript()
    If waves == None || IsEventFinished()
        Return
    EndIf
    String regularID = "Wave 1: Wolves"
    If aiWaveNumber == 2
        regularID = "Wave 2: Bloodbugs"
    ElseIf aiWaveNumber == 3
        regularID = "Wave 3: Yao Guai"
    EndIf
    Int waveIndex = waves.FindEncounterWaveIndex(regularID)
    Int rustlerIndex = waves.FindEncounterWaveIndex("Wave " + aiWaveNumber + ": Raider Rustlers (P56 Update)")
    If rustlerIndex >= 0 && (waveIndex < 0 || Utility.RandomInt(0, 1) == 1)
        waveIndex = rustlerIndex
    EndIf
    waves.StartEncounterWave(waveIndex)
EndFunction

Function StartBossFight()
    DefaultQuestEncounterWaveScript waves = GetWaveScript()
    If waves == None || IsEventFinished()
        Return
    EndIf
    PlayScareEffects()
    waves.StartEncounterWaveByID("Boss: Sheepsquatch")
    waves.StartEncounterWaveByID("Boss Adds: Bloatflies & Stingwings")
EndFunction

Function StopHerdWaves()
    DefaultQuestEncounterWaveScript waves = GetWaveScript()
    If waves != None
        waves.StopAllEncounterWaves(False)
    EndIf
EndFunction

Function PlayScareEffects()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    If QSTSheepsquatchHowl != None
        QSTSheepsquatchHowl.Play(playerRef)
    EndIf
    If E01B_Herd_CameraShakeSpell != None && EventQuestScript != None && EventQuestScript.IsPlayerParticipating()
        E01B_Herd_CameraShakeSpell.Cast(playerRef, playerRef)
    EndIf
EndFunction

Function ScheduleShutdown(Float afDelay)
    CancelTimer(49860)
    CancelTimer(49861)
    StartTimer(afDelay, 49861)
EndFunction

Function CleanupEvent()
    StopHerdWaves()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && E01B_Herd_Perk != None && playerRef.HasPerk(E01B_Herd_Perk)
        playerRef.RemovePerk(E01B_Herd_Perk)
    EndIf
    If E01B_Herd_Equip != None && E01B_Herd_Equip.IsRunning()
        E01B_Herd_Equip.Stop()
    EndIf

    Int index = 0
    While Brahmin_Aliases != None && index < Brahmin_Aliases.Length
        Actor brahmin = GetBrahmin(index)
        If brahmin != None
            If E01B_Herd_ScaredBrahminKeyword != None
                brahmin.RemoveKeyword(E01B_Herd_ScaredBrahminKeyword)
            EndIf
            If E01B_Herd_BrahminTravelKeyword != None
                ObjectReference travelMarker = brahmin.GetLinkedRef(E01B_Herd_BrahminTravelKeyword)
                If travelMarker != None
                    brahmin.SetLinkedRef(None, E01B_Herd_BrahminTravelKeyword)
                    travelMarker.Delete()
                EndIf
            EndIf
            brahmin.Delete()
        EndIf
        index += 1
    EndWhile
    If Alias_Actor_Handy != None && Alias_Actor_Handy.GetReference() != None
        Alias_Actor_Handy.GetReference().Delete()
    EndIf
EndFunction
