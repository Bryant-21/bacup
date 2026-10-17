Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    If KeywordToSend == None
        Return
    EndIf
    Bool started = KeywordToSend.SendStoryEventAndWait(None, akSpeakerRef)
    If akSpeakerRef == None
        Return
    EndIf
    If started && StartUpSucceededTopic != None
        akSpeakerRef.Say(StartUpSucceededTopic)
    ElseIf !started && StartUpFailedTopic != None
        akSpeakerRef.Say(StartUpFailedTopic)
    EndIf
EndEvent
