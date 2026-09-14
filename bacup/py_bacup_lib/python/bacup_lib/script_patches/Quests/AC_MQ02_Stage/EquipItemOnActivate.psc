Event OnActivate(ObjectReference akActionRef)
    Actor player = akActionRef as Actor
    If player == None || player != Game.GetPlayer() || WeaponToEquip == None
        Return
    EndIf
    If player.GetItemCount(WeaponToEquip) < 1
        player.AddItem(WeaponToEquip, 1, True)
    EndIf
    player.EquipItem(WeaponToEquip, False, True)
    If WeaponEquippedMessage != None
        WeaponEquippedMessage.Show()
    EndIf
EndEvent
