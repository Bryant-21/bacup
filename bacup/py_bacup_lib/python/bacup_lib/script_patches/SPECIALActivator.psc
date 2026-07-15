Event OnActivate(ObjectReference akActionRef)
    Actor activatingActor = akActionRef as Actor
    If activatingActor == None
        Return
    EndIf

    If !bAllowRepetition && BlockingActorValue != None && activatingActor.GetValue(BlockingActorValue) >= 1.0
        If SPECIALFailureMessage != None
            SPECIALFailureMessage.Show()
        EndIf
        Return
    EndIf

    If SPECIALTestInitialMessage != None
        If SPECIALTestInitialMessage.Show() != iMessageButtonIndex
            Return
        EndIf
    EndIf

    If SpecialActorValue != None && activatingActor.GetValue(SpecialActorValue) < iSpecialRank
        If SPECIALFailureMessage != None
            SPECIALFailureMessage.Show()
        EndIf
        Return
    EndIf

    If QuestToStart != None
        QuestToStart.SendStoryEvent(None, Self, activatingActor, iSpecialRank)
    EndIf
    If !bAllowRepetition && BlockingActorValue != None
        activatingActor.SetValue(BlockingActorValue, 1.0)
    EndIf
EndEvent
