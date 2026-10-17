; Lucky Strike. The map sends MTRZ05MapKeyword with its tier (1 Miner, 2 Prospector, 3 Excavator) as aiValue1.
; FO76's stage-255 reward (QuestReward_MTRZ05_Lucky_Stage255_01) gated each tier's loot and caps on iRewardValue,
; a quest-variable condition B21:QuestRewards cannot evaluate, so the tier reward is granted here; B21:QuestRewards
; still pays the shared base caps.
Event OnStoryScript(Keyword akKeyword, Location akLocation, ObjectReference akRef1, ObjectReference akRef2, Int aiValue1, Int aiValue2)
    If aiValue1 >= 1 && aiValue1 <= 3
        iRewardValue = aiValue1
    Else
        iRewardValue = TierFromPlayerMapValue()
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 255
        GrantTierReward()
        ClearPlayerMapValues()
    EndIf
EndEvent

Int Function TierFromPlayerMapValue()
    Actor player = Game.GetPlayer()
    If MTRz05Map03Value != None && player.GetValue(MTRz05Map03Value) > 0.0
        Return 3
    ElseIf MTRz05Map02Value != None && player.GetValue(MTRz05Map02Value) > 0.0
        Return 2
    EndIf
    Return 1
EndFunction

Function GrantTierReward()
    Actor player = MTRz05Player.GetActorReference()
    If player == None
        player = Game.GetPlayer()
    EndIf
    Int tier = iRewardValue
    If tier < 1 || tier > 3
        tier = 1
    EndIf
    Int mineLoot = 0x32AD5F
    Int capsGlobal = 0x86AEEA
    If tier == 2
        mineLoot = 0x32AD60
        capsGlobal = 0x8AC559
    ElseIf tier == 3
        mineLoot = 0x32AD61
        capsGlobal = 0x8AC55A
    EndIf
    AddRewardItem(player, mineLoot)
    AddRewardItem(player, 0x43934D)
    AddRewardItem(player, 0x7AC791)
    GlobalVariable caps = Game.GetFormFromFile(capsGlobal, "SeventySix.esm") as GlobalVariable
    If caps != None && caps.GetValueInt() > 0
        player.AddItem(Game.GetCaps(), caps.GetValueInt())
    EndIf
EndFunction

Function AddRewardItem(Actor akPlayer, Int aiFormID)
    Form item = Game.GetFormFromFile(aiFormID, "SeventySix.esm")
    If item != None
        akPlayer.AddItem(item, 1)
    EndIf
EndFunction

Function ClearPlayerMapValues()
    Actor player = Game.GetPlayer()
    If MTRz05Map01Value != None
        player.SetValue(MTRz05Map01Value, 0.0)
    EndIf
    If MTRz05Map02Value != None
        player.SetValue(MTRz05Map02Value, 0.0)
    EndIf
    If MTRz05Map03Value != None
        player.SetValue(MTRz05Map03Value, 0.0)
    EndIf
EndFunction
