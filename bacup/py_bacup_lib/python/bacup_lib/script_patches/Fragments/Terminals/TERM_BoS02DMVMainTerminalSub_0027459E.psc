Function Fragment_Terminal_01()
    If pBoS02 == None || pBoS02.GetStage() >= 600
        Return
    EndIf
    pBoS02.SetStage(561)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If pBoS02DMV_AV != None
            playerRef.SetValue(pBoS02DMV_AV, 1.0)
        EndIf
        If pBoS02_ApplicationForm != None && playerRef.GetItemCount(pBoS02_ApplicationForm) == 0
            playerRef.AddItem(pBoS02_ApplicationForm, 1, False)
        EndIf
    EndIf
    pBoS02.SetStage(600)
EndFunction

Function Fragment_Terminal_02()
    If pBoS02 == None || pBoS02.GetStage() >= 600
        Return
    EndIf
    pBoS02.SetStage(562)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If pBoS02DMV_AV != None
            playerRef.SetValue(pBoS02DMV_AV, 1.0)
        EndIf
        If pBoS02_ApplicationForm != None && playerRef.GetItemCount(pBoS02_ApplicationForm) == 0
            playerRef.AddItem(pBoS02_ApplicationForm, 1, False)
        EndIf
    EndIf
    pBoS02.SetStage(600)
EndFunction

Function Fragment_Terminal_03()
    If pBoS02 == None || pBoS02.GetStage() >= 600
        Return
    EndIf
    pBoS02.SetStage(563)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If pBoS02DMV_AV != None
            playerRef.SetValue(pBoS02DMV_AV, 1.0)
        EndIf
        If pBoS02_ApplicationForm != None && playerRef.GetItemCount(pBoS02_ApplicationForm) == 0
            playerRef.AddItem(pBoS02_ApplicationForm, 1, False)
        EndIf
    EndIf
    pBoS02.SetStage(600)
EndFunction

Function Fragment_Terminal_04()
    If pBoS02 == None || pBoS02.GetStage() >= 600
        Return
    EndIf
    pBoS02.SetStage(564)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If pBoS02DMV_AV != None
            playerRef.SetValue(pBoS02DMV_AV, 1.0)
        EndIf
        If pBoS02_ApplicationForm != None && playerRef.GetItemCount(pBoS02_ApplicationForm) == 0
            playerRef.AddItem(pBoS02_ApplicationForm, 1, False)
        EndIf
    EndIf
    pBoS02.SetStage(600)
EndFunction
