Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget == Game.GetPlayer() && FadeToBlack != None
        FadeToBlack.Apply(1.0)
    EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If akTarget == Game.GetPlayer()
        W05_MQR_203P_QuestScript questScript = W05_MQR_203P as W05_MQR_203P_QuestScript
        ObjectReference destinationRef
        If questScript != None && questScript.SlaveQuartersMarker != None
            destinationRef = questScript.SlaveQuartersMarker.GetReference()
        EndIf
        If destinationRef != None
            akTarget.MoveTo(destinationRef)
        EndIf
        If WakeUp != None
            WakeUp.Apply(1.0)
        EndIf
        If destinationRef != None && W05_MQR_203P != None && !W05_MQR_203P.IsStageDone(1900)
            W05_MQR_203P.SetStage(1900)
        EndIf
    EndIf
EndEvent
