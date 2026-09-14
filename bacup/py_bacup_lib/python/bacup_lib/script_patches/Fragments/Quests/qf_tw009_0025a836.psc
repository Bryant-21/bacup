Function Fragment_Stage_0500_Item_00()
    ObjectReference robotRef = Alias_ConfederateGutsy01.GetReference()
    If robotRef != None
        robotRef.Enable(False)
    EndIf
    robotRef = Alias_ConfederateGutsy02.GetReference()
    If robotRef != None
        robotRef.Enable(False)
    EndIf
    robotRef = Alias_ConfederateGutsy03.GetReference()
    If robotRef != None
        robotRef.Enable(False)
    EndIf
    robotRef = Alias_ConfederateGutsy04.GetReference()
    If robotRef != None
        robotRef.Enable(False)
    EndIf
    robotRef = Alias_ConfederateGutsy05.GetReference()
    If robotRef != None
        robotRef.Enable(False)
    EndIf
EndFunction
