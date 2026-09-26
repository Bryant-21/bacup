Event OnRead()
    If MapMarkerToAdd != None
        MapMarkerToAdd.AddToMap(False)
    EndIf
    Quest masterQuest = Game.GetFormFromFile(0x003CD064, "SeventySix.esm") as Quest
    Nuke_MasterScript master = masterQuest as Nuke_MasterScript
    If master != None && B21:KeypadNative.Ready()
        Debug.MessageBox(master.DescribeLocalCodePiece(GetBaseObject()))
    EndIf
EndEvent
