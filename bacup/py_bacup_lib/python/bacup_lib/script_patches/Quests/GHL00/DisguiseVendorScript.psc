; Jaye Vo's disguise service. GHL_DisguiseOutfit is GHL_LL_Disguise_Clothes (the Drifter Outfit) plus
; GHL_LL_Disguise_Headgear (one of three sack hoods); FO4 cannot read outfit parts, so the resolved items are looked up
; directly. Dressing up gives and equips the outfit and a hood; undressing strips and takes back every disguise item.
Armor[] Function GetDisguiseHoods()
    Armor[] hoods = new Armor[3]
    hoods[0] = Game.GetFormFromFile(0x007B77F6, "SeventySix.esm") as Armor
    hoods[1] = Game.GetFormFromFile(0x007B77F8, "SeventySix.esm") as Armor
    hoods[2] = Game.GetFormFromFile(0x007B77FA, "SeventySix.esm") as Armor
    Return hoods
EndFunction

Function EquipDisguise()
    Actor player = Game.GetPlayer()
    Armor clothes = Game.GetFormFromFile(0x007C92C4, "SeventySix.esm") as Armor
    If clothes != None
        If player.GetItemCount(clothes) == 0
            player.AddItem(clothes, 1, True)
        EndIf
        player.EquipItem(clothes, False, True)
    EndIf
    Armor[] hoods = GetDisguiseHoods()
    Armor hood = None
    Int index = 0
    While index < hoods.Length && hood == None
        If hoods[index] != None && player.GetItemCount(hoods[index]) > 0
            hood = hoods[index]
        EndIf
        index += 1
    EndWhile
    If hood == None
        hood = hoods[Utility.RandomInt(0, hoods.Length - 1)]
        If hood != None
            player.AddItem(hood, 1, True)
        EndIf
    EndIf
    If hood != None
        player.EquipItem(hood, False, True)
    EndIf
    GHL00_EquippedDisguiseMessage.Show()
EndFunction

Function RemoveDisguise()
    Actor player = Game.GetPlayer()
    Armor[] hoods = GetDisguiseHoods()
    Armor clothes = Game.GetFormFromFile(0x007C92C4, "SeventySix.esm") as Armor
    hoods.Add(clothes)
    Int index = 0
    While index < hoods.Length
        Int count = 0
        If hoods[index] != None
            count = player.GetItemCount(hoods[index])
        EndIf
        If count > 0
            player.UnequipItem(hoods[index], False, True)
            player.RemoveItem(hoods[index], count, True)
        EndIf
        index += 1
    EndWhile
    GHL00_RemovedDisguiseMessage.Show()
EndFunction
