; "Here is your checked item." only plays while the player holds a claim ticket; it hands over the item.
Function Fragment_Begin(ObjectReference akSpeakerRef)
    If Alias_ScenePlayer != None && Game.GetPlayer() != None
        Alias_ScenePlayer.ForceRefTo(Game.GetPlayer())
    EndIf
    MTNL01BellhopQuestScript bellhop = GetOwningQuest() as MTNL01BellhopQuestScript
    If bellhop != None
        bellhop.ClaimCheckedItem()
    EndIf
EndFunction
