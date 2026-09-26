; Bound to LuresToOrient, the malfunctioning lures.
Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    If akSenderRef == None || akActionRef == None || akActionRef != Game.GetPlayer()
        Return
    EndIf
    ENs02_BlastQuestScript controller = GetOwningQuest() as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_LureOriented(akSenderRef)
    EndIf
EndEvent
