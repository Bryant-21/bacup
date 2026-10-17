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

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction
