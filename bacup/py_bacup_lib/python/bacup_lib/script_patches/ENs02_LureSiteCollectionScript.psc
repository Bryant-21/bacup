; Bound to LuresObj, the lures still awaiting initialization. The first
; activation also releases the cargobot when the join stage never fired.
Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    If akSenderRef == None || akActionRef == None || akActionRef != Game.GetPlayer()
        Return
    EndIf
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning() || owner.IsStopping()
        Return
    EndIf
    If iVBFailsafeStage >= 0 && !owner.IsStageDone(iVBFailsafeStage)
        owner.SetStage(iVBFailsafeStage)
    EndIf
    ENs02_BlastQuestScript controller = owner as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_LureInitialized(akSenderRef)
    EndIf
EndEvent
