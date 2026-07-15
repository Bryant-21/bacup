Function SendConfiguredStoryEvent()
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
    MyStoryManagerKeyword.SendStoryEvent(eventLocation, eventReference, None, iValue1, iValue2)
EndFunction
