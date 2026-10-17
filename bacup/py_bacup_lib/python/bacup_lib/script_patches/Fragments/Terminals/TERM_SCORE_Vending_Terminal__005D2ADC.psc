Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    B21:AmmoConverter.Trade(akTerminalRef, Ammo5mm, AmmoConverter_5mm_Ammo_Count, AmmoConverter_5mm_Ammo_Point_BuyCost, 1, True)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    B21:AmmoConverter.Trade(akTerminalRef, Ammo5mm, AmmoConverter_5mm_Ammo_Count, AmmoConverter_5mm_Ammo_Point_BuyCost, 10, True)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    B21:AmmoConverter.Trade(akTerminalRef, Ammo5mm, AmmoConverter_5mm_Ammo_Count, AmmoConverter_5mm_Ammo_Point_BuyCost, 100, True)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    B21:AmmoConverter.Trade(akTerminalRef, Ammo5mm, AmmoConverter_5mm_Ammo_Count, AmmoConverter_5mm_Ammo_Point_SellGain, 1, False)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    B21:AmmoConverter.Trade(akTerminalRef, Ammo5mm, AmmoConverter_5mm_Ammo_Count, AmmoConverter_5mm_Ammo_Point_SellGain, 10, False)
EndFunction

Function Fragment_Terminal_07(ObjectReference akTerminalRef)
    B21:AmmoConverter.Trade(akTerminalRef, Ammo5mm, AmmoConverter_5mm_Ammo_Count, AmmoConverter_5mm_Ammo_Point_SellGain, 100, False)
EndFunction
