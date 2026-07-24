Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget == Game.GetPlayer()
        FadeToBlack.Apply(1.0)
    EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If akTarget == Game.GetPlayer()
        WakeUp.Apply(1.0)

        W05_MQR_203P_QuestScript questScript = W05_MQR_203P as W05_MQR_203P_QuestScript
        If questScript != None && questScript.SlaveQuartersMarker != None
            ObjectReference destinationRef = questScript.SlaveQuartersMarker.GetReference()
            If destinationRef != None
                akTarget.MoveTo(destinationRef)
                If !W05_MQR_203P.IsStageDone(1900)
                    W05_MQR_203P.SetStage(1900)
                EndIf
            EndIf
        EndIf
    EndIf
EndEvent
