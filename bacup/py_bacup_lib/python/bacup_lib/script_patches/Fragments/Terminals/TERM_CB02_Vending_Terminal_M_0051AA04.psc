; Prices are the menu texts: Pencil 10, Pen 20, Yard Stick 25, Scissors 40, Wonder Glue 50.
; Item 5 (BobbyPinBox) has no menu entry. Item 8 deposits the carried candy.
Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    (akTerminalRef.GetBaseObject() as CB02_VendingTerminalScript).Purchase(akTerminalRef, Pencil, 10, False)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    (akTerminalRef.GetBaseObject() as CB02_VendingTerminalScript).Purchase(akTerminalRef, Pen01, 20, False)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    (akTerminalRef.GetBaseObject() as CB02_VendingTerminalScript).Purchase(akTerminalRef, Yardstick, 25, False)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    (akTerminalRef.GetBaseObject() as CB02_VendingTerminalScript).Purchase(akTerminalRef, Scissors, 40, False)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    (akTerminalRef.GetBaseObject() as CB02_VendingTerminalScript).Purchase(akTerminalRef, Wonderglue, 50, False)
EndFunction

Function Fragment_Terminal_08(ObjectReference akTerminalRef)
    (akTerminalRef.GetBaseObject() as CB02_VendingTerminalScript).DepositCandy(akTerminalRef)
EndFunction
