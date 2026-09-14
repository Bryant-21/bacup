Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == iStageClearHostiles
        PrepareMezzanine()
    ElseIf auiStageID == 1050
        CheckMezzanineHostiles()
    ElseIf auiStageID == 1110
        SpawnMegParty()
    EndIf
EndEvent

Function PrepareMezzanine()
    ObjectReference enableMarker = EnableMarkerAtToTW.GetReference()
    If enableMarker
        enableMarker.Enable()
    EndIf
    Int index = 0
    While index < ToTWActorRefs.GetCount()
        Actor actorRef = ToTWActorRefs.GetAt(index) as Actor
        If actorRef
            actorRef.Enable()
            actorRef.EvaluatePackage()
        EndIf
        index += 1
    EndWhile
EndFunction

Function CheckMezzanineHostiles()
    Int livingHostiles = 0
    Int index = 0
    While index < MezzanineHostiles.GetCount()
        Actor hostileRef = MezzanineHostiles.GetAt(index) as Actor
        If hostileRef && !hostileRef.IsDead()
            livingHostiles += 1
            RegisterForRemoteEvent(hostileRef, "OnDeath")
        EndIf
        index += 1
    EndWhile
    If livingHostiles == 0 && !IsStageDone(iStageSpawnMeg)
        SetStage(iStageSpawnMeg)
    EndIf
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    CheckMezzanineHostiles()
EndEvent

Function SpawnMegParty()
    Actor megRef = MegAtToTW.GetActorReference()
    Actor raiderARef = RaiderAAtToTW.GetActorReference()
    Actor raiderBRef = RaiderBAtToTW.GetActorReference()
    If megRef
        megRef.Enable()
        megRef.EvaluatePackage()
    EndIf
    If raiderARef
        raiderARef.Enable()
        raiderARef.EvaluatePackage()
    EndIf
    If raiderBRef
        raiderBRef.Enable()
        raiderBRef.EvaluatePackage()
    EndIf
EndFunction
