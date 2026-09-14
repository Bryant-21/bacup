Quest Function OwningQuest()
    If myQI == None
        myQI = GetOwningQuest()
    EndIf
    Return myQI
EndFunction

Bool Function CanAdvance()
    Quest owner = OwningQuest()
    If owner == None
        Return False
    EndIf
    If PrereqStage > 0 && !owner.IsStageDone(PrereqStage)
        Return False
    EndIf
    If ShutoffStage > 0 && owner.IsStageDone(ShutoffStage)
        Return False
    EndIf
    Return True
EndFunction

Event OnAliasInit()
    OwningQuest()
EndEvent

Event OnSit(ObjectReference akFurniture)
    If akFurniture != None && CanAdvance()
        OwningQuest().SetStage(StageToSet)
    EndIf
EndEvent
