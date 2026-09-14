Event OnActivate(ObjectReference akActionRef)
    Actor player = akActionRef as Actor
    If player == None || player != Game.GetPlayer()
        Return
    EndIf
    If ThrowKnifeFromMarker != None
        player.MoveTo(ThrowKnifeFromMarker)
    EndIf
    If player.GetItemCount(AC_MQ02_Stage_ThrowingKnife_Weapon) < 1
        player.AddItem(AC_MQ02_Stage_ThrowingKnife_Weapon, 1, True)
    EndIf
    player.EquipItem(AC_MQ02_Stage_ThrowingKnife_Weapon, False, True)
EndEvent
