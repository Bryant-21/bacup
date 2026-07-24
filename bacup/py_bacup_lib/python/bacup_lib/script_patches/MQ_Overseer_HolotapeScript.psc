Event OnEquipped(Actor akActor)
    If akActor != Game.GetPlayer()
        Return
    EndIf

    CacheContainer.Lock(false)
    If akActor.GetItemCount(RecipeToTeach) == 0
        akActor.AddItem(RecipeToTeach, 1)
    EndIf
EndEvent
