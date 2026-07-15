Event OnActivate(ObjectReference akActionRef)
    Actor inspectingActor = akActionRef as Actor
    If inspectingActor != Game.GetPlayer()
        Return
    EndIf

    If Messages != None
        Int messageIndex = 0
        While messageIndex < Messages.Length
            If Messages[messageIndex].TargetAV != None && \
                    inspectingActor.GetValue(Messages[messageIndex].TargetAV) >= Messages[messageIndex].RequiredAmount
                If Messages[messageIndex].ShownMessage != None
                    Messages[messageIndex].ShownMessage.Show()
                    Return
                EndIf
            EndIf
            messageIndex += 1
        EndWhile
    EndIf

    If FallbackMessage != None
        FallbackMessage.Show()
    EndIf
EndEvent
