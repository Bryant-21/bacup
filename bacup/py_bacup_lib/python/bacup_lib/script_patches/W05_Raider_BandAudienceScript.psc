Event OnInit()
    If AudienceTrigger != None && AudienceTrigger.GetReference() != None
        RegisterForRemoteEvent(AudienceTrigger.GetReference(), "OnTriggerEnter")
    EndIf
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If !CanSayLine
        Return
    EndIf
    If Audience == None || Audience.GetCount() <= 0
        Return
    EndIf

    ObjectReference speaker = Audience.GetAt(Utility.RandomInt(0, Audience.GetCount() - 1))
    If speaker == None
        Return
    EndIf

    If W05_Crater_Talent != None && W05_Crater_Talent.GetValue() == 1.0
        If W05_RaiderCheerLines != None
            speaker.SayCustom(W05_RaiderCheerLines)
        EndIf
    Else
        If W05_RaiderJeerLines != None
            speaker.SayCustom(W05_RaiderJeerLines)
        EndIf
    EndIf
EndEvent
