; Prices are the menu texts. The toy lists hold toys; the surprises hold ammo, a
; quest-reward weapon or nuclear material, and legendary base items, which trip the
; machine's "not on the approved registry" warning on the confirmation page.
Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    (akTerminalRef.GetBaseObject() as CB02_VendingTerminalScript).Purchase(akTerminalRef, CB02_LL_VendingList_SmallToy, 50, False)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    (akTerminalRef.GetBaseObject() as CB02_VendingTerminalScript).Purchase(akTerminalRef, CB02_LL_VendingList_MediumToy, 75, False)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    (akTerminalRef.GetBaseObject() as CB02_VendingTerminalScript).Purchase(akTerminalRef, CB02_LL_VendingList_LargeToy, 125, False)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    (akTerminalRef.GetBaseObject() as CB02_VendingTerminalScript).Purchase(akTerminalRef, CB02_LL_VendingList_Surprise1, 400, True)
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    (akTerminalRef.GetBaseObject() as CB02_VendingTerminalScript).Purchase(akTerminalRef, CB02_LL_VendingList_Surprise2, 800, True)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    (akTerminalRef.GetBaseObject() as CB02_VendingTerminalScript).Purchase(akTerminalRef, LGND_PossibleLegendaryItemBaseLists, 2000, True)
EndFunction
