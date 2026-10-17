Event OnStart(Actor akActor)
    If bRunOnStart
        TrySetStage()
    EndIf
EndEvent

Event OnEnd(Actor akActor)
    If bRunOnEnd
        TrySetStage()
    EndIf
EndEvent

Event OnChange(Actor akActor)
    If bRunOnChange
        TrySetStage()
    EndIf
EndEvent

Function TrySetStage()
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning() || iStageToSet < 0 || owner.IsStageDone(iStageToSet)
        Return
    EndIf
    If iPreReqStage >= 0 && !owner.IsStageDone(iPreReqStage)
        Return
    EndIf
    If iTurnOffStage >= 0 && owner.IsStageDone(iTurnOffStage)
        Return
    EndIf
    owner.SetStage(iStageToSet)
EndFunction
