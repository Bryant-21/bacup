Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef && GotKit && playerRef.GetValue(GotKit) < 1.0
        playerRef.AddItem(Food0, 1, False)
        playerRef.AddItem(Food1, 1, False)
        playerRef.AddItem(Food2, 1, False)
        playerRef.AddItem(Food3, 1, False)
        playerRef.AddItem(Food4, 1, False)
        playerRef.AddItem(Food5, 1, False)
        playerRef.AddItem(Food6, 1, False)
        playerRef.AddItem(Water, 1, False)
        playerRef.SetValue(GotKit, 1.0)
    EndIf
EndFunction
