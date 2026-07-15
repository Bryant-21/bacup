"""Static preset constants for weapon toggle variables, common events, and standard transitions."""

WEAPON_TOGGLE_VARIABLES = [
    {"variableName": "fToggleBlend", "variableType": 4, "variableValue": "0",
     "variableMinValue": "0", "variableMaxValue": "1"},
    {"variableName": "fToggleBlendDampened", "variableType": 4, "variableValue": "0",
     "variableMinValue": "0", "variableMaxValue": "1"},
    {"variableName": "fDampRate", "variableType": 4, "variableValue": "0.949999988079071",
     "variableMinValue": "0", "variableMaxValue": "0.99000000953674316"},
    {"variableName": "fCoolTimer", "variableType": 4, "variableValue": "0",
     "variableMinValue": "0", "variableMaxValue": "0"},
]

WEAPON_TOGGLE_EVENTS = [
    "SoundPlay", "SoundStop", "WeaponFire", "attackStartAuto",
    "attackStateExit", "CoolDown00", "end",
]

STANDARD_TRANSITIONS = [
    {"transitionName": "zeroDuration", "transitionDuration": "0",
     "transitionSelfTransitionMode": 0, "transitionEventMode": 0,
     "transitionFlags": 0, "transitionEndMode": 0,
     "transitionBlendCurve": 0, "transitionVariableBindingSet": 0,
     "transitionToGeneratorStartTimeFraction": "0"},
    {"transitionName": "halfSecondBlend", "transitionDuration": "0.5",
     "transitionSelfTransitionMode": 0, "transitionEventMode": 0,
     "transitionFlags": 0, "transitionEndMode": 0,
     "transitionBlendCurve": 0, "transitionVariableBindingSet": 0,
     "transitionToGeneratorStartTimeFraction": "0"},
]
