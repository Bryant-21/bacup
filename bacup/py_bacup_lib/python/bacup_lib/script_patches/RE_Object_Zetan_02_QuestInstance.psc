; RE_Object_Zetan_02 alien tea party. Shooting the cymbal monkey trap (TrapCymbalMonkey, one of RefAliases) is
; StageToDismember ("Monkey shot"), which blows the seated aliens apart.
Event OnQuestInit()
    count = -1
    Int index = 0
    While RefAliases != None && index < RefAliases.Length
        curObject = RefAliases[index].GetReference()
        If curObject != None && (curObject as Actor) == None
            RegisterForHitEvent(curObject)
        EndIf
        index += 1
    EndWhile
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    If StageToDismember >= 0 && !IsStageDone(StageToDismember)
        SetStage(StageToDismember)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID != StageToDismember || RefAliases == None
        Return
    EndIf
    UnregisterForAllHitEvents()
    count = 0
    Int index = 0
    While index < RefAliases.Length
        curActor = RefAliases[index].GetActorReference()
        If curActor != None
            curActor.Kill()
            curActor.Dismember("Head1", True, True, True)
            count += 1
        EndIf
        index += 1
    EndWhile
    curActor = None
    curObject = None
EndEvent
