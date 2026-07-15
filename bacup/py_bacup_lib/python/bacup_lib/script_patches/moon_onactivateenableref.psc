Event OnActivate(ObjectReference akActionRef)
    Actor activatingPlayer = akActionRef as Actor
    If activatingPlayer != Game.GetPlayer()
        Return
    EndIf

    If activatingPlayer.GetItemCount(inventoryItemToCheck) < numItemsRequiredToPlace
        If tryToPlaceWithoutExplosiveMessage != None
            tryToPlaceWithoutExplosiveMessage.Show()
        EndIf
        If tryToPlaceWithoutExplosiveSound != None && !activatingPlayer.HasKeyword(MOON_Ambush_Keyword_SoundCooldown)
            tryToPlaceWithoutExplosiveSound.Play(Self)
            activatingPlayer.AddSpell(MOON_Ambush_SPLL_InteractionSoundCooldown, False)
        EndIf
        Return
    EndIf

    activatingPlayer.RemoveItem(inventoryItemToCheck, numItemsRequiredToPlace)
    If objectToEnable != None
        objectToEnable.Enable()
    EndIf
    If placedBombSound != None
        placedBombSound.Play(Self)
    EndIf
    If placedBombMessage != None
        placedBombMessage.Show()
    EndIf
EndEvent
