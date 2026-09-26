Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    If pBoS03 != None && !pBoS03.IsStageDone(600)
        pBoS03.SetStage(600)
    EndIf
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If B21UltracitePlansDownloaded || playerRef == None
        Return
    EndIf
    Form[] plans = new Form[6]
    plans[0] = Game.GetFormFromFile(0x005018A3, "SeventySix.esm")
    plans[1] = Game.GetFormFromFile(0x005018A4, "SeventySix.esm")
    plans[2] = Game.GetFormFromFile(0x005018A5, "SeventySix.esm")
    plans[3] = Game.GetFormFromFile(0x005018A6, "SeventySix.esm")
    plans[4] = Game.GetFormFromFile(0x005018A7, "SeventySix.esm")
    plans[5] = Game.GetFormFromFile(0x005018A8, "SeventySix.esm")
    Int index = 0
    While index < plans.Length
        If plans[index] == None
            Return
        EndIf
        index += 1
    EndWhile
    B21UltracitePlansDownloaded = True
    index = 0
    While index < plans.Length
        If playerRef.GetItemCount(plans[index]) == 0
            playerRef.AddItem(plans[index], 1, True)
        EndIf
        index += 1
    EndWhile
    Debug.Notification("Ultracite schematics downloaded. Read the plans in your inventory to learn them.")
EndFunction
