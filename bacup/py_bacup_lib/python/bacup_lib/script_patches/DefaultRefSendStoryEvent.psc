; akRef2 is optional so the existing no-argument callers are unaffected; it exists for
; DefaultOnReadSendStoryEvent's SendContainerAsRef2, the only carrier that has a second
; reference worth sending.
Function SendConfiguredStoryEvent(ObjectReference akRef2 = None)
    If MyStoryManagerKeyword == None
        Return
    EndIf

    Location eventLocation = akLoc
    If eventLocation == None
        eventLocation = GetCurrentLocation()
    EndIf

    ObjectReference eventReference = akRef1
    If eventReference == None
        eventReference = Self
    EndIf

    If ShowTraces
        Debug.Trace(Self + " sending story event " + MyStoryManagerKeyword)
    EndIf
    MyStoryManagerKeyword.SendStoryEvent(eventLocation, eventReference, akRef2, iValue1, iValue2)
EndFunction
