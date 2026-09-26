CB02_QuestScript Function ParentScript()
    If CB02_QuestScriptIns == None && CB02_MonsterMash != None
        CB02_QuestScriptIns = CB02_MonsterMash as CB02_QuestScript
    EndIf
    Return CB02_QuestScriptIns
EndFunction

Bool Function IsMaskTeamOver()
    If IsStopping() || IsStopped() || IsCompleted()
        Return True
    EndIf
    If StopRespawningCandyBucketGuard >= 0 && IsStageDone(StopRespawningCandyBucketGuard)
        Return True
    EndIf
    Return CB02_MonsterMash != None && !CB02_MonsterMash.IsRunning()
EndFunction

ObjectReference Function PickCandySpawnMarker()
    If CandySpawnMarkers == None || CandySpawnMarkers.GetCount() <= 0
        Return None
    EndIf
    Int markerCount = CandySpawnMarkers.GetCount()
    Int attempt = 0
    While attempt < 4
        ObjectReference marker = CandySpawnMarkers.GetAt(Utility.RandomInt(0, markerCount - 1))
        If marker != None && (marker != LastSpawnMarkerRef || markerCount == 1)
            Return marker
        EndIf
        attempt += 1
    EndWhile
    Return CandySpawnMarkers.GetAt(0)
EndFunction

Function RemoveCandyBucket()
    If CandyBucketRef != None
        UnregisterForRemoteEvent(CandyBucketRef, "OnActivate")
        CandyBucketRef.DisableNoWait()
        CandyBucketRef.Delete()
        CandyBucketRef = None
    EndIf
    If CandyBucket != None
        CandyBucket.Clear()
    EndIf
EndFunction

Function SpawnCandyBucket()
    If IsMaskTeamOver() || CB02_CandyBucket == None
        Return
    EndIf
    ObjectReference marker = PickCandySpawnMarker()
    If marker == None
        StartTimer(5.0, 51001)
        Return
    EndIf

    RemoveCandyBucket()
    CandyBucketRef = marker.PlaceAtMe(CB02_CandyBucket, 1, True, False, False)
    If CandyBucketRef == None
        StartTimer(5.0, 51001)
        Return
    EndIf
    LastSpawnMarkerRef = marker
    If CandyBucket != None
        CandyBucket.ForceRefTo(CandyBucketRef)
    EndIf
    RegisterForRemoteEvent(CandyBucketRef, "OnActivate")
    If GetCandyBucketObjective >= 0
        SetObjectiveCompleted(GetCandyBucketObjective, False)
        SetObjectiveDisplayed(GetCandyBucketObjective, True, True)
    EndIf
    CB02_QuestScript parentScript = ParentScript()
    If parentScript != None
        parentScript.AnnounceCandyBucket(True)
    EndIf
    ; The first bucket also spawns its guards through DefaultSimpleRespawnScript.
    If SpawnCandyBucketStage >= 0 && !IsStageDone(SpawnCandyBucketStage)
        SetStage(SpawnCandyBucketStage)
    EndIf
EndFunction

Event OnQuestInit()
    CandyBucketRef = None
    LastSpawnMarkerRef = None
    ParentScript()
    If CB02_MonsterMash != None
        RegisterForRemoteEvent(CB02_MonsterMash, "OnStageSet")
    EndIf
    SpawnCandyBucket()
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender != CB02_MonsterMash || auiStageID != ParentQuestCompleteStage
        Return
    EndIf
    If StopRespawningCandyBucketGuard >= 0 && !IsStageDone(StopRespawningCandyBucketGuard)
        SetStage(StopRespawningCandyBucketGuard)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If StopRespawningCandyBucketGuard >= 0 && auiStageID == StopRespawningCandyBucketGuard
        CancelTimer(51001)
        RemoveCandyBucket()
        If GetCandyBucketObjective >= 0 && IsObjectiveDisplayed(GetCandyBucketObjective)
            SetObjectiveCompleted(GetCandyBucketObjective, True)
        EndIf
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akSender != CandyBucketRef || akActionRef != Game.GetPlayer()
        Return
    EndIf
    CB02_QuestScript parentScript = ParentScript()
    If parentScript != None
        parentScript.RefillMaskFromBucket()
        parentScript.AnnounceCandyBucket(False)
    EndIf
    RemoveCandyBucket()
    If !IsMaskTeamOver()
        ; A collected bucket reappears elsewhere in the school.
        StartTimer(15.0, 51001)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 51001
        SpawnCandyBucket()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(51001)
    RemoveCandyBucket()
    If CB02_MonsterMash != None
        UnregisterForRemoteEvent(CB02_MonsterMash, "OnStageSet")
    EndIf
EndEvent
